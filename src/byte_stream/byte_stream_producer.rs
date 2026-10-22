use anyhow::Result;

use crate::byte_stream::{SeekableByteStream, SyncByteStream};
use crate::producer::ThreadPoolProducer;

/// Creates a [ThreadPoolProducer] from a [SyncByteStream]
pub fn new_byte_stream_thread_pool_producer<S: 'static + SyncByteStream + Send>(
    block_size: usize,
    buffer_num_blocks: usize,
    num_workers: usize,
    make_byte_stream: impl Fn() -> Result<S>,
) -> Result<ThreadPoolProducer<Vec<u8>>> {
    ThreadPoolProducer::new(num_workers, buffer_num_blocks, move || {
        let mut byte_stream = make_byte_stream()?;
        // A block is just whatever the byte stream outputs next, no matter the block's index
        Ok(move |_index| {
            // TODO Can we avoid the zero initialization of the buffer?
            let mut buffer = vec![0; block_size];
            byte_stream.blocking_read(buffer.as_mut()).unwrap();
            Ok(buffer)
        })
    })
}

/// Creates a [ThreadPoolProducer] from a [SeekableByteStream]. Its blocks are consecutive parts
/// of the stream, beginning at `start_position`. Since blocks are received in order, a receiver
/// gets exactly what the stream outputs, no matter how many workers there are.
///
/// For this to work, `make_byte_stream` has to return the same stream each time it is called.
pub fn new_seekable_byte_stream_thread_pool_producer<S: 'static + SeekableByteStream + Send>(
    block_size: usize,
    buffer_num_blocks: usize,
    num_workers: usize,
    start_position: u64,
    make_byte_stream: impl Fn() -> Result<S>,
) -> Result<ThreadPoolProducer<Vec<u8>>> {
    ThreadPoolProducer::new(num_workers, buffer_num_blocks, move || {
        let mut byte_stream = make_byte_stream()?;
        Ok(move |index: u64| {
            // TODO Can we avoid the zero initialization of the buffer?
            let mut buffer = vec![0; block_size];
            byte_stream.seek(start_position + index * block_size as u64)?;
            byte_stream.blocking_read(buffer.as_mut())?;
            Ok(buffer)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_stream::testutils::FakeByteStream;
    use crate::producer::{Producer, ProductReceiver};

    const NUM_BLOCKS: usize = 20;

    fn stream_content(start_position: u64, num_bytes: usize) -> Vec<u8> {
        let mut stream = FakeByteStream::new(0);
        stream.seek(start_position).unwrap();
        let mut content = vec![0; num_bytes];
        stream.blocking_read(&mut content).unwrap();
        content
    }

    fn produced_blocks(num_workers: usize, block_size: usize, start_position: u64) -> Vec<u8> {
        let producer = new_seekable_byte_stream_thread_pool_producer(
            block_size,
            5,
            num_workers,
            start_position,
            || Ok(FakeByteStream::new(0)),
        )
        .unwrap();
        let receiver = producer.make_receiver();
        (0..NUM_BLOCKS)
            .flat_map(|_| receiver.blocking_get_product().unwrap())
            .collect()
    }

    #[test]
    fn seekable_blocks_have_block_size() {
        let producer = new_seekable_byte_stream_thread_pool_producer(1234, 5, 3, 0, || {
            Ok(FakeByteStream::new(0))
        })
        .unwrap();
        let receiver = producer.make_receiver();

        for _ in 0..NUM_BLOCKS {
            assert_eq!(1234, receiver.blocking_get_product().unwrap().len());
        }
    }

    #[test]
    fn seekable_blocks_are_the_stream_no_matter_the_number_of_workers() {
        let expected = stream_content(0, NUM_BLOCKS * 4096);
        for num_workers in [1, 2, 7] {
            assert!(
                expected == produced_blocks(num_workers, 4096, 0),
                "Blocks of {num_workers} workers aren't the stream"
            );
        }
    }

    #[test]
    fn seekable_blocks_are_the_stream_no_matter_the_block_size() {
        // Block sizes that are and that aren't multiples of the 64 bytes the cipher works with
        for block_size in [1, 64, 100, 4096, 65_537] {
            assert!(
                stream_content(0, NUM_BLOCKS * block_size) == produced_blocks(4, block_size, 0),
                "Blocks of size {block_size} aren't the stream"
            );
        }
    }

    #[test]
    fn seekable_blocks_begin_at_start_position() {
        for start_position in [1, 64, 12_345, 1 << 40] {
            assert!(
                stream_content(start_position, NUM_BLOCKS * 1000)
                    == produced_blocks(4, 1000, start_position),
                "Blocks don't begin at position {start_position} of the stream"
            );
        }
    }
}
