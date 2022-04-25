use anyhow::{ensure, Result};
use async_trait::async_trait;
use std::collections::VecDeque;

use super::block_source::{AsyncBlockSource, SyncBlockSource};
use super::{AsyncByteStream, SyncByteStream};

/// Creates a [SyncByteStream] or [AsyncByteStream] from a [SyncBlockSource] or [AsyncBlockSource] that will output bytes
/// from blocks returned by that block source.
/// If there are other consumers of the same block source, then some blocks will go to this byte stream and some
/// will go to other consumers, so there is no guarantee that the byte stream will yield exactly all of the blocks
/// produced by the block source.
pub struct BlockSourceByteStream<B> {
    block_source: B,
    buffer: Buffer,
}

impl<B> BlockSourceByteStream<B> {
    pub fn new(block_source: B) -> Self {
        Self {
            block_source,
            buffer: Buffer::new(),
        }
    }
}

impl<B: SyncBlockSource> BlockSourceByteStream<B> {
    fn _fill_buffer_sync(&mut self, min_buffer_size: usize) -> Result<()>
    where
        B: SyncBlockSource,
    {
        while self.buffer.len() < min_buffer_size {
            let next_block = self.block_source.blocking_read()?;
            self.buffer.push(next_block);
        }
        Ok(())
    }
}

impl<B: SyncBlockSource> SyncByteStream for BlockSourceByteStream<B> {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        self._fill_buffer_sync(dest.len())?;
        self.buffer.pop(dest).expect(
            "Not enough bytes in buffer. This should be impossible because we just filled it.",
        );
        Ok(())
    }
}

impl<B: AsyncBlockSource + Send> BlockSourceByteStream<B> {
    async fn _fill_buffer_async(&mut self, min_buffer_size: usize) -> Result<()> {
        while self.buffer.len() < min_buffer_size {
            let next_block = self.block_source.async_read().await?;
            self.buffer.push(next_block);
        }
        Ok(())
    }
}

#[async_trait]
impl<B: AsyncBlockSource + Send> AsyncByteStream for BlockSourceByteStream<B> {
    async fn async_read(&mut self, dest: &mut [u8]) -> Result<()> {
        self._fill_buffer_async(dest.len()).await?;
        self.buffer.pop(dest).expect(
            "Not enough bytes in buffer. This should be impossible because we just filled it.",
        );
        Ok(())
    }
}
struct Buffer {
    buffer: VecDeque<Vec<u8>>,

    // Invariant: If buffer has an entry, then current_pos < buffer[0].len()
    current_pos: usize,

    // Invariant: buffer_size = buffer.map(|b| b.len()).sum()
    buffer_size: usize,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            current_pos: 0,
            buffer_size: 0,
        }
    }

    pub fn push(&mut self, data: Vec<u8>) {
        self.buffer_size += data.len();
        self.buffer.push_back(data);
    }

    pub fn pop(&mut self, mut dest: &mut [u8]) -> Result<()> {
        ensure!(self.buffer_size >= dest.len(), "Not enough bytes in buffer");
        while dest.len() > 0 {
            dest = self._fill_from_first(dest);
        }
        Ok(())
    }

    // Take bytes from the first buffer block and put them into dest, returning
    // the tail of dest that still remains to be filled.
    // This also advances self.current_pos so mark those bytes as used, and if
    // there aren't any bytes left in the first buffer block, it removes that block
    // from the buffer.
    fn _fill_from_first<'a>(&mut self, dest: &'a mut [u8]) -> &'a mut [u8] {
        assert!(self.buffer_size >= dest.len(), "Not enough bytes in buffer");
        assert!(dest.len() > 0, "Precondition violated");

        if let Some(front) = self.buffer.front() {
            let bytes_available = &front[self.current_pos..];
            assert!(bytes_available.len() > 0, "Miscalculated self.current_pos");
            let copy_num_bytes = bytes_available.len().min(dest.len());
            dest[..copy_num_bytes].copy_from_slice(&bytes_available[..copy_num_bytes]);
            self.current_pos += copy_num_bytes;
            self.buffer_size -= copy_num_bytes;
            if self.current_pos == front.len() {
                self.buffer.pop_front();
                self.current_pos = 0;
            }
            &mut dest[copy_num_bytes..]
        } else {
            panic!(
                "Buffer is empty. Can't happen since we asserted above that there's enough space."
            );
        }
    }

    pub fn len(&self) -> usize {
        self.buffer_size
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Check the buffer_size invariant. If it's correct now, it probably was correct all the time.
        let actual_buffer_size: usize =
            self.buffer.iter().map(|b| b.len()).sum::<usize>() - self.current_pos as usize;
        assert_eq!(
            self.buffer_size, actual_buffer_size,
            "Miscalculated self.buffer_size"
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use super::*;
    use crate::byte_stream::block_source::testutils::FakeBlockSource;
    use crate::byte_stream::testutils::FakeByteStream;
    use futures::join;

    fn assert_sync_byte_stream_eq(
        mut lhs: impl SyncByteStream,
        mut rhs: impl SyncByteStream,
        check_num_bytes: usize,
    ) {
        const BLOCK_SIZE: usize = 1234; // Use non-2^x block size to make sure block boundaries don't align with our block source
        let check_num_blocks = (check_num_bytes / BLOCK_SIZE) + 1;
        for _ in 0..check_num_blocks {
            let mut lhs_block = [0; BLOCK_SIZE];
            lhs.blocking_read(&mut lhs_block).unwrap();
            let mut rhs_block = [0; BLOCK_SIZE];
            rhs.blocking_read(&mut rhs_block).unwrap();
            assert_eq!(hex::encode(lhs_block), hex::encode(rhs_block));
        }
    }

    async fn assert_async_byte_stream_eq(
        mut lhs: impl AsyncByteStream,
        mut rhs: impl AsyncByteStream,
        check_num_bytes: usize,
    ) {
        const BLOCK_SIZE: usize = 1234; // Use non-2^x block size to make sure block boundaries don't align with our block source
        let check_num_blocks = (check_num_bytes / BLOCK_SIZE) + 1;
        for _ in 0..check_num_blocks {
            let mut lhs_block = [0; BLOCK_SIZE];
            let mut rhs_block = [0; BLOCK_SIZE];
            let (lhs_result, rhs_result) = join!(
                lhs.async_read(&mut lhs_block),
                rhs.async_read(&mut rhs_block),
            );
            lhs_result.unwrap();
            rhs_result.unwrap();
            assert_eq!(lhs_block, rhs_block);
        }
    }

    fn test_sync_stream_is_correct(block_size: usize) {
        let expected_byte_stream = FakeByteStream::new(0);
        let actual_byte_stream = BlockSourceByteStream::new(FakeBlockSource::new(block_size, 0));
        assert_sync_byte_stream_eq(expected_byte_stream, actual_byte_stream, 10 * block_size);
    }

    async fn test_async_stream_is_correct(block_size: usize) {
        let expected_byte_stream = FakeByteStream::new(0);
        let actual_byte_stream = BlockSourceByteStream::new(FakeBlockSource::new(block_size, 0));
        assert_async_byte_stream_eq(expected_byte_stream, actual_byte_stream, 10 * block_size)
            .await;
    }

    #[test]
    fn test_givenBlockSize1_thenSyncStreamIsCorrect() {
        test_sync_stream_is_correct(1);
    }

    #[tokio::test]
    async fn test_givenBlockSize1_thenAsyncStreamIsCorrect() {
        test_async_stream_is_correct(1).await;
    }

    #[test]
    fn test_givenBlockSize10_thenSyncStreamIsCorrect() {
        test_sync_stream_is_correct(10);
    }

    #[tokio::test]
    async fn test_givenBlockSize10_thenAsyncStreamIsCorrect() {
        test_async_stream_is_correct(10).await;
    }

    #[test]
    fn test_givenBlockSize100_thenSyncStreamIsCorrect() {
        test_sync_stream_is_correct(100);
    }

    #[tokio::test]
    async fn test_givenBlockSize100_thenAsyncStreamIsCorrect() {
        test_async_stream_is_correct(100).await;
    }

    #[test]
    fn test_givenBlockSize10000_thenSyncStreamIsCorrect() {
        test_sync_stream_is_correct(10000);
    }

    #[tokio::test]
    async fn test_givenBlockSize10000_thenAsyncStreamIsCorrect() {
        test_async_stream_is_correct(10000).await;
    }

    #[test]
    fn test_givenBlockSize1M_thenSyncStreamIsCorrect() {
        test_sync_stream_is_correct(1024 * 1024);
    }

    #[tokio::test]
    async fn test_givenBlockSize1M_thenAsyncStreamIsCorrect() {
        test_async_stream_is_correct(1024 * 1024).await;
    }
}
