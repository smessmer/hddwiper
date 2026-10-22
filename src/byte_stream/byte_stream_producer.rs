use anyhow::Result;
use std::ops::DerefMut;

use crate::byte_stream::{SeekableByteStream, SyncByteStream};
use crate::producer::ThreadPoolProducer;

/// Creates a [ThreadPoolProducer] from a [SyncByteStream]. Each block is generated in the
/// memory that `new_block` returns for it, which gets the size of the block.
pub fn new_byte_stream_thread_pool_producer<S, B>(
    block_size: usize,
    new_block: fn(usize) -> B,
    buffer_num_blocks: usize,
    num_workers: usize,
    make_byte_stream: impl Fn() -> Result<S>,
) -> Result<ThreadPoolProducer<B>>
where
    S: 'static + SyncByteStream + Send,
    B: 'static + DerefMut<Target = [u8]> + Send,
{
    ThreadPoolProducer::new(num_workers, buffer_num_blocks, move || {
        let mut byte_stream = make_byte_stream()?;
        // A block is just whatever the byte stream outputs next, no matter the block's index
        Ok(move |_index| {
            // TODO Can we avoid the zero initialization of the buffer?
            let mut buffer = new_block(block_size);
            byte_stream.blocking_read(&mut buffer).unwrap();
            Ok(buffer)
        })
    })
}

/// Creates a [ThreadPoolProducer] from a [SeekableByteStream]. Its blocks are consecutive parts
/// of the stream, beginning at `start_position`. Since blocks are received in order, a receiver
/// gets exactly what the stream outputs, no matter how many workers there are. Each block is
/// generated in the memory that `new_block` returns for it, which gets the size of the block.
///
/// For this to work, `make_byte_stream` has to return the same stream each time it is called.
pub fn new_seekable_byte_stream_thread_pool_producer<S, B>(
    block_size: usize,
    new_block: fn(usize) -> B,
    buffer_num_blocks: usize,
    num_workers: usize,
    start_position: u64,
    make_byte_stream: impl Fn() -> Result<S>,
) -> Result<ThreadPoolProducer<B>>
where
    S: 'static + SeekableByteStream + Send,
    B: 'static + DerefMut<Target = [u8]> + Send,
{
    ThreadPoolProducer::new(num_workers, buffer_num_blocks, move || {
        let mut byte_stream = make_byte_stream()?;
        Ok(move |index: u64| {
            // TODO Can we avoid the zero initialization of the buffer?
            let mut buffer = new_block(block_size);
            byte_stream.seek(start_position + index * block_size as u64)?;
            byte_stream.blocking_read(&mut buffer)?;
            Ok(buffer)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aligned_buffer::{AlignedBuffer, ALIGNMENT};
    use crate::byte_stream::testutils::FakeByteStream;
    use crate::producer::{Producer, ProductReceiver};
    use anyhow::anyhow;
    use std::sync::atomic::{AtomicU64, Ordering};

    const NUM_BLOCKS: usize = 20;

    fn new_vec(size: usize) -> Vec<u8> {
        vec![0; size]
    }

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
            new_vec,
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

    /// The first [NUM_BLOCKS] blocks of the stream with `seed`
    fn stream_blocks(seed: u64, block_size: usize) -> Vec<Vec<u8>> {
        let mut stream = FakeByteStream::new(seed);
        (0..NUM_BLOCKS)
            .map(|_| {
                let mut block = vec![0; block_size];
                stream.blocking_read(&mut block).unwrap();
                block
            })
            .collect()
    }

    #[test]
    fn blocks_have_block_size() {
        let producer = new_byte_stream_thread_pool_producer(1234, new_vec, 5, 3, || {
            Ok(FakeByteStream::new(0))
        })
        .unwrap();
        let receiver = producer.make_receiver();

        for _ in 0..NUM_BLOCKS {
            assert_eq!(1234, receiver.blocking_get_product().unwrap().len());
        }
    }

    #[test]
    fn blocks_of_one_worker_are_its_stream() {
        for block_size in [1, 64, 100, 4096] {
            let producer = new_byte_stream_thread_pool_producer(block_size, new_vec, 5, 1, || {
                Ok(FakeByteStream::new(0))
            })
            .unwrap();
            let receiver = producer.make_receiver();

            for (index, expected) in stream_blocks(0, block_size).into_iter().enumerate() {
                assert!(
                    expected == receiver.blocking_get_product().unwrap(),
                    "Block {index} of size {block_size} isn't the next part of the stream"
                );
            }
        }
    }

    #[test]
    fn each_worker_reads_its_own_stream() {
        const NUM_WORKERS: u64 = 4;
        const BLOCK_SIZE: usize = 1000;

        // Each worker gets a stream with another seed
        let num_streams = AtomicU64::new(0);
        let producer = new_byte_stream_thread_pool_producer(
            BLOCK_SIZE,
            new_vec,
            5,
            NUM_WORKERS as usize,
            || {
                Ok(FakeByteStream::new(
                    num_streams.fetch_add(1, Ordering::SeqCst),
                ))
            },
        )
        .unwrap();
        assert_eq!(NUM_WORKERS, num_streams.load(Ordering::SeqCst));
        let receiver = producer.make_receiver();

        // Each block has to be the next block of one of the streams. So no worker skips a part
        // of its stream or uses it twice, although the blocks of all workers are mixed.
        let streams: Vec<Vec<Vec<u8>>> = (0..NUM_WORKERS)
            .map(|seed| stream_blocks(seed, BLOCK_SIZE))
            .collect();
        let mut num_blocks_taken = vec![0; streams.len()];
        for index in 0..NUM_BLOCKS {
            let block = receiver.blocking_get_product().unwrap();
            let stream = (0..streams.len())
                .find(|&stream| streams[stream].get(num_blocks_taken[stream]) == Some(&block))
                .unwrap_or_else(|| panic!("Block {index} isn't the next block of any stream"));
            num_blocks_taken[stream] += 1;
        }
    }

    #[test]
    fn fails_if_the_stream_cant_be_made() {
        let result = new_byte_stream_thread_pool_producer(1000, new_vec, 5, 3, || {
            Err::<FakeByteStream, _>(anyhow!("Simulated failure"))
        });

        assert_eq!(
            "Simulated failure",
            result.map(|_| ()).unwrap_err().to_string()
        );
    }

    #[test]
    fn seekable_blocks_have_block_size() {
        let producer =
            new_seekable_byte_stream_thread_pool_producer(1234, new_vec, 5, 3, 0, || {
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

    #[test]
    fn blocks_are_generated_in_the_memory_of_new_block() {
        // A size that isn't whole sectors, so that it only depends on the memory what is aligned
        const BLOCK_SIZE: usize = 5000;
        let expected = stream_blocks(0, BLOCK_SIZE);

        let producer =
            new_byte_stream_thread_pool_producer(BLOCK_SIZE, AlignedBuffer::new, 5, 1, || {
                Ok(FakeByteStream::new(0))
            })
            .unwrap();
        let receiver = producer.make_receiver();
        let blocks: Vec<AlignedBuffer> = (0..NUM_BLOCKS)
            .map(|_| receiver.blocking_get_product().unwrap())
            .collect();

        for (index, block) in blocks.iter().enumerate() {
            assert_eq!(0, block.as_ptr() as usize % ALIGNMENT);
            assert!(
                expected[index][..] == block[..],
                "Wrong data in block {index}"
            );
        }
    }

    #[test]
    fn seekable_blocks_are_generated_in_the_memory_of_new_block() {
        const BLOCK_SIZE: usize = 5000;
        let expected = stream_content(12_345, NUM_BLOCKS * BLOCK_SIZE);

        let producer = new_seekable_byte_stream_thread_pool_producer(
            BLOCK_SIZE,
            AlignedBuffer::new,
            5,
            4,
            12_345,
            || Ok(FakeByteStream::new(0)),
        )
        .unwrap();
        let receiver = producer.make_receiver();
        let blocks: Vec<AlignedBuffer> = (0..NUM_BLOCKS)
            .map(|_| receiver.blocking_get_product().unwrap())
            .collect();

        for (block, expected) in blocks.iter().zip(expected.chunks(BLOCK_SIZE)) {
            assert_eq!(0, block.as_ptr() as usize % ALIGNMENT);
            assert!(expected == &block[..]);
        }
    }
}
