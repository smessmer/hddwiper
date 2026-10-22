use std::io::{ErrorKind, IoSlice, Write};
use std::ops::Deref;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

use crate::producer::ProductReceiver;

/// Launches a thread and writes everything from a [BlockSource] into a [Writer], e.g. a [std::fs::File].
pub struct BlockWriter {
    join_handle: JoinHandle<()>,
    num_bytes_written: Arc<AtomicU64>,
}

impl BlockWriter {
    /// Launch a new thread that writes blocks from `block_source` into `writer` until the `writer` is EOF
    /// or the [BlockWriter] gets cancelled. The first `num_bytes_to_skip` bytes of the blocks
    /// aren't written.
    pub fn new<B: 'static + Deref<Target = [u8]>>(
        block_source: impl 'static + ProductReceiver<B> + Send,
        writer: impl 'static + Write + Send,
        num_bytes_to_skip: usize,
    ) -> Self {
        let num_bytes_written = Arc::new(0.into());
        let join_handle = _launch_worker_thread(
            block_source,
            writer,
            num_bytes_to_skip,
            Arc::clone(&num_bytes_written),
        );
        Self {
            join_handle,
            num_bytes_written,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }

    pub fn num_bytes_written(&self) -> u64 {
        self.num_bytes_written.load(Ordering::Relaxed)
    }

    pub fn join(self) {
        self.join_handle.join().unwrap();
    }
}

fn _launch_worker_thread<B: 'static + Deref<Target = [u8]>>(
    block_source: impl 'static + ProductReceiver<B> + Send,
    mut writer: impl 'static + Write + Send,
    mut num_bytes_to_skip: usize,
    num_bytes_written: Arc<AtomicU64>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            log::debug!("Getting blocks...");
            // Block until at least one product is available, then drain the rest.
            // Without the blocking_get_product, get_all_available_products can return
            // an empty vec when the RNG is slower than the disk, causing a hot busy-loop
            // that wastes CPU cycles that should be spent generating random data.
            let first_block = match block_source.blocking_get_product() {
                Ok(block) => block,
                Err(_) => return,
            };
            let mut blocks = block_source.get_all_available_products();
            blocks.insert(0, first_block);
            let mut io_slices: Vec<IoSlice> = blocks
                .iter()
                .map(|block| {
                    let num_skipped = num_bytes_to_skip.min(block.len());
                    num_bytes_to_skip -= num_skipped;
                    IoSlice::new(&block[num_skipped..])
                })
                .collect();
            // Writing them changes the slices, so we have to count before
            let size_to_write: u64 = io_slices.iter().map(|slice| slice.len() as u64).sum();
            log::debug!("Getting blocks...writing block...");
            let write_result = writer.write_all_vectored(&mut io_slices);
            if let Err(err) = &write_result {
                if err.kind() == ErrorKind::StorageFull {
                    return;
                } else {
                    write_result.unwrap();
                }
            }
            num_bytes_written.fetch_add(size_to_write, Ordering::Relaxed);
            log::debug!("Getting block...writing block...finished");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::io::{self, Cursor};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// A mock ProductReceiver that returns a fixed number of blocks then stops
    struct MockBlockSource {
        blocks: Arc<Mutex<Vec<Vec<u8>>>>,

        /// Whether it hands out its blocks one at a time, so that each is written on its own
        one_at_a_time: bool,
    }

    impl MockBlockSource {
        fn new(blocks: Vec<Vec<u8>>) -> Self {
            Self {
                blocks: Arc::new(Mutex::new(blocks)),
                one_at_a_time: false,
            }
        }

        fn one_at_a_time(blocks: Vec<Vec<u8>>) -> Self {
            Self {
                one_at_a_time: true,
                ..Self::new(blocks)
            }
        }
    }

    impl ProductReceiver<Vec<u8>> for MockBlockSource {
        fn blocking_get_product(&self) -> Result<Vec<u8>> {
            loop {
                let mut blocks = self.blocks.lock().unwrap();
                if !blocks.is_empty() {
                    return Ok(blocks.remove(0));
                }
                drop(blocks);
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        fn get_all_available_products(&self) -> Vec<Vec<u8>> {
            if self.one_at_a_time {
                return vec![];
            }
            let mut blocks = self.blocks.lock().unwrap();
            std::mem::take(&mut *blocks)
        }

        fn num_products_in_buffer(&self) -> usize {
            self.blocks.lock().unwrap().len()
        }
    }

    /// A writer that stores written data and can simulate storage full
    struct TestWriter {
        data: Arc<Mutex<Vec<u8>>>,
        max_bytes: Option<usize>,
    }

    impl TestWriter {
        fn new() -> Self {
            Self {
                data: Arc::new(Mutex::new(Vec::new())),
                max_bytes: None,
            }
        }

        fn with_max_bytes(max_bytes: usize) -> Self {
            Self {
                data: Arc::new(Mutex::new(Vec::new())),
                max_bytes: Some(max_bytes),
            }
        }
    }

    impl Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut data = self.data.lock().unwrap();
            if let Some(max) = self.max_bytes {
                if data.len() >= max {
                    return Err(io::Error::new(ErrorKind::StorageFull, "Storage full"));
                }
                let available = max - data.len();
                let to_write = buf.len().min(available);
                data.extend_from_slice(&buf[..to_write]);
                if to_write < buf.len() {
                    return Err(io::Error::new(ErrorKind::StorageFull, "Storage full"));
                }
                Ok(to_write)
            } else {
                data.extend_from_slice(buf);
                Ok(buf.len())
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn writes_blocks_to_writer() {
        let blocks = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        let source = MockBlockSource::new(blocks);
        let writer = TestWriter::new();
        let writer_data = Arc::clone(&writer.data);

        let block_writer = BlockWriter::new(source, writer, 0);

        // Wait for writer to process
        std::thread::sleep(Duration::from_millis(50));

        // Get the written data
        let data = writer_data.lock().unwrap();
        assert_eq!(*data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

        // num_bytes_written should reflect written data
        assert_eq!(block_writer.num_bytes_written(), 9);
    }

    /// Waits until `block_writer` wrote `num_bytes` bytes, and fails the test if that takes
    /// too long
    fn wait_until_written(block_writer: &BlockWriter, num_bytes: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while block_writer.num_bytes_written() < num_bytes {
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for the writer"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn skips_the_beginning_of_the_first_block() {
        let blocks = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        let source = MockBlockSource::new(blocks);
        let writer = TestWriter::new();
        let writer_data = Arc::clone(&writer.data);

        let block_writer = BlockWriter::new(source, writer, 2);
        wait_until_written(&block_writer, 7);

        assert_eq!(*writer_data.lock().unwrap(), vec![3, 4, 5, 6, 7, 8, 9]);
        // What was skipped doesn't count as written
        assert_eq!(block_writer.num_bytes_written(), 7);
    }

    #[test]
    fn skips_more_than_one_block() {
        // The whole first block, a part of the second one, and nothing of what follows
        let blocks = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
            vec![10, 11, 12],
        ];
        let source = MockBlockSource::new(blocks);
        let writer = TestWriter::new();
        let writer_data = Arc::clone(&writer.data);

        let block_writer = BlockWriter::new(source, writer, 4);
        wait_until_written(&block_writer, 8);

        assert_eq!(
            *writer_data.lock().unwrap(),
            vec![5, 6, 7, 8, 9, 10, 11, 12]
        );
        assert_eq!(block_writer.num_bytes_written(), 8);
    }

    #[test]
    fn skips_blocks_that_arrive_one_by_one() {
        let blocks = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        let source = MockBlockSource::one_at_a_time(blocks);
        let writer = TestWriter::new();
        let writer_data = Arc::clone(&writer.data);

        // What is left to skip has to be remembered from one block to the next
        let block_writer = BlockWriter::new(source, writer, 5);
        wait_until_written(&block_writer, 4);

        assert_eq!(*writer_data.lock().unwrap(), vec![6, 7, 8, 9]);
    }

    #[test]
    fn stops_on_storage_full() {
        let blocks = vec![vec![1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10]];
        let source = MockBlockSource::new(blocks);
        let writer = TestWriter::with_max_bytes(7);

        let block_writer = BlockWriter::new(source, writer, 0);

        // Wait for writer to hit storage full and finish
        std::thread::sleep(Duration::from_millis(100));

        assert!(block_writer.is_finished());
    }

    /// A writer that can't write anything, like a device that is broken
    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("Simulated failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    #[should_panic]
    fn panics_when_writing_fails() {
        let source = MockBlockSource::new(vec![vec![1, 2, 3]]);

        let block_writer = BlockWriter::new(source, BrokenWriter, 0);

        // Only a full device ends a wipe quietly. Every other error is a crash, which is how the
        // user gets to know that the device couldn't be written.
        block_writer.join();
    }

    #[test]
    fn is_finished_returns_false_while_running() {
        // Create a source that keeps providing blocks
        let blocks: Vec<Vec<u8>> = (0..100).map(|i| vec![i; 100]).collect();
        let source = MockBlockSource::new(blocks);
        let writer = Cursor::new(Vec::new());

        let block_writer = BlockWriter::new(source, writer, 0);

        // Should not be finished immediately
        assert!(!block_writer.is_finished());
    }

    #[test]
    fn num_bytes_written_updates_correctly() {
        let blocks = vec![vec![0; 100], vec![0; 200], vec![0; 300]];
        let source = MockBlockSource::new(blocks);
        let writer = TestWriter::new();

        let block_writer = BlockWriter::new(source, writer, 0);

        // Wait for all blocks to be written
        std::thread::sleep(Duration::from_millis(50));

        assert_eq!(block_writer.num_bytes_written(), 600);
    }
}
