use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, PoisonError};

use crate::aligned_buffer::AlignedBuffer;

/// Hands out the memory for blocks, and gets the memory of each block back when that block isn't
/// needed anymore, for the blocks it hands out after that.
///
/// A wipe of a very fast device needs more than a thousand blocks per second, but only a few of
/// them at the same time. New memory for each of them is expensive: The system has to find and
/// clear it when it is used for the first time, and when memory is given back, all threads have
/// to be told that it is gone. With many threads that generate blocks, that took more time than
/// writing the blocks. Huge pages, see [AlignedBuffer], are even more expensive to get, and the
/// system doesn't always have one.
#[derive(Clone, Default)]
pub struct BlockPool {
    unused_buffers: Arc<Mutex<Vec<AlignedBuffer>>>,
}

impl BlockPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a block of `size` bytes. What it contains is whatever an earlier block of that
    /// size contained, or zeroes, so it has to be filled completely before it is used.
    pub fn block(&self, size: usize) -> PooledBlock {
        let reused_buffer = {
            let mut unused_buffers = self.unused_buffers.lock().unwrap();
            // Usually all blocks have the same size. The buffer that was used last is the one
            // whose memory the processor most likely still knows.
            unused_buffers
                .iter()
                .rposition(|buffer| buffer.len() == size)
                .map(|index| unused_buffers.swap_remove(index))
        };

        PooledBlock {
            buffer: Some(reused_buffer.unwrap_or_else(|| AlignedBuffer::new(size))),
            unused_buffers: Arc::clone(&self.unused_buffers),
        }
    }
}

/// A block from a [BlockPool]. When it is dropped, the pool gets its memory back.
pub struct PooledBlock {
    /// Only taken out when the block is dropped
    buffer: Option<AlignedBuffer>,

    /// The buffers of the pool this block is from
    unused_buffers: Arc<Mutex<Vec<AlignedBuffer>>>,
}

impl Deref for PooledBlock {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.buffer.as_ref().unwrap()
    }
}

impl DerefMut for PooledBlock {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.buffer.as_mut().unwrap()
    }
}

impl Drop for PooledBlock {
    fn drop(&mut self) {
        let buffer = self.buffer.take().unwrap();
        // If another thread panicked, the list of buffers is still fine, because nothing that
        // is done to it while it is locked can fail halfway
        self.unused_buffers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aligned_buffer::ALIGNMENT;
    use std::collections::HashSet;

    const SIZE: usize = 4 * ALIGNMENT;

    fn address(block: &PooledBlock) -> usize {
        block.as_ptr() as usize
    }

    #[test]
    fn blocks_have_the_size_and_are_aligned() {
        let pool = BlockPool::new();

        // A block of each size twice, so that the second one reuses the memory of the first
        for size in [ALIGNMENT, SIZE, 1, 1000, 0, ALIGNMENT, SIZE, 1, 1000, 0] {
            let block = pool.block(size);

            assert_eq!(size, block.len());
            assert_eq!(0, address(&block) % ALIGNMENT);
        }
    }

    #[test]
    fn new_block_consists_of_zeroes_and_can_be_changed() {
        let pool = BlockPool::new();
        let mut block = pool.block(SIZE);
        assert!(block.iter().all(|byte| *byte == 0));

        block[0] = 1;
        block[SIZE - 1] = 2;

        assert_eq!(1, block[0]);
        assert_eq!(2, block[SIZE - 1]);
        assert!(block[1..SIZE - 1].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn block_reuses_the_memory_of_a_dropped_block() {
        let pool = BlockPool::new();
        let mut first = pool.block(SIZE);
        first.fill(7);
        let first_address = address(&first);
        drop(first);

        let second = pool.block(SIZE);

        assert_eq!(first_address, address(&second));
        // Nobody cleared it in between, which is the point of reusing it
        assert!(second.iter().all(|byte| *byte == 7));
    }

    #[test]
    fn blocks_that_exist_at_the_same_time_have_memory_of_their_own() {
        let pool = BlockPool::new();
        let mut blocks: Vec<PooledBlock> = (0..5).map(|_| pool.block(SIZE)).collect();

        for (index, block) in blocks.iter_mut().enumerate() {
            block.fill(index as u8);
        }

        for (index, block) in blocks.iter().enumerate() {
            assert!(block.iter().all(|byte| *byte == index as u8));
        }
        let addresses: HashSet<usize> = blocks.iter().map(address).collect();
        assert_eq!(5, addresses.len());
    }

    #[test]
    fn needs_no_more_memory_than_the_blocks_that_exist_at_the_same_time() {
        let pool = BlockPool::new();
        let mut addresses = HashSet::new();

        // Like a wipe: there are always a few blocks, and the oldest one is dropped
        let mut blocks: Vec<PooledBlock> = (0..3).map(|_| pool.block(SIZE)).collect();
        addresses.extend(blocks.iter().map(address));
        for _ in 0..100 {
            blocks.remove(0);
            blocks.push(pool.block(SIZE));
            addresses.extend(blocks.iter().map(address));
        }

        assert_eq!(3, addresses.len());
    }

    #[test]
    fn reuses_the_memory_that_was_used_last() {
        let pool = BlockPool::new();
        let blocks: Vec<PooledBlock> = (0..3).map(|_| pool.block(SIZE)).collect();
        let addresses: Vec<usize> = blocks.iter().map(address).collect();
        // They are dropped in this order
        drop(blocks);

        for expected in addresses.iter().rev() {
            // Keeping the block, so that the next one gets other memory
            let block = pool.block(SIZE);
            assert_eq!(*expected, address(&block));
            std::mem::forget(block);
        }
    }

    #[test]
    fn block_of_another_size_doesnt_get_the_memory_of_a_dropped_block() {
        let pool = BlockPool::new();
        let mut small = pool.block(SIZE);
        small.fill(7);
        let small_address = address(&small);
        drop(small);

        let large = pool.block(2 * SIZE);
        assert_eq!(2 * SIZE, large.len());
        assert!(large.iter().all(|byte| *byte == 0));

        // And the memory of the small block is still there for a block of its size, also
        // when a block of another size was dropped after it
        drop(large);
        let small_again = pool.block(SIZE);
        assert_eq!(small_address, address(&small_again));
        assert!(small_again.iter().all(|byte| *byte == 7));
    }

    #[test]
    fn clones_share_their_memory() {
        let pool = BlockPool::new();
        let first_address = address(&pool.clone().block(SIZE));

        assert_eq!(first_address, address(&pool.block(SIZE)));
        assert_eq!(first_address, address(&pool.clone().block(SIZE)));
    }

    #[test]
    fn pools_dont_share_their_memory() {
        let pool = BlockPool::new();
        let mut block = pool.block(SIZE);
        block.fill(7);
        drop(block);

        let other_block = BlockPool::new().block(SIZE);

        assert!(other_block.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn block_can_outlive_its_pool() {
        let pool = BlockPool::new();
        let mut block = pool.block(SIZE);
        drop(pool);

        block.fill(7);

        assert!(block.iter().all(|byte| *byte == 7));
    }

    #[test]
    fn blocks_can_be_taken_and_dropped_by_several_threads() {
        const NUM_THREADS: usize = 8;
        const NUM_BLOCKS: usize = 2000;
        let pool = BlockPool::new();

        // Each thread fills its blocks with a value of its own. If two blocks ever shared
        // their memory, one of them would find the value of another thread in it.
        let threads: Vec<_> = (0..NUM_THREADS)
            .map(|thread_index| {
                let pool = pool.clone();
                std::thread::spawn(move || {
                    let mut held: Vec<PooledBlock> = Vec::new();
                    for block_index in 0..NUM_BLOCKS {
                        let mut block = pool.block(SIZE);
                        block.fill(thread_index as u8);
                        held.push(block);
                        if block_index % 3 == 0 {
                            held.remove(0);
                        }
                        for block in &held[held.len().saturating_sub(4)..] {
                            assert!(block.iter().all(|byte| *byte == thread_index as u8));
                        }
                        if held.len() > 8 {
                            held.clear();
                        }
                    }
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn block_is_returned_even_if_another_thread_panicked_with_the_pool() {
        let pool = BlockPool::new();
        let block = pool.block(SIZE);
        let block_address = address(&block);

        // Poisons the lock of the pool
        let unused_buffers = Arc::clone(&pool.unused_buffers);
        let _ = std::thread::spawn(move || {
            let _guard = unused_buffers.lock().unwrap();
            panic!("This panic is a part of the test");
        })
        .join();
        assert!(pool.unused_buffers.is_poisoned());

        // This must not panic, it also happens while a thread that panicked is cleaned up
        drop(block);

        let unused_buffers = pool
            .unused_buffers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(1, unused_buffers.len());
        assert_eq!(block_address, unused_buffers[0].as_ptr() as usize);
    }
}
