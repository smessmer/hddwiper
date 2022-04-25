use std::io::{ErrorKind, IoSlice, Write};
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
    /// or the [BlockWriter] gets cancelled
    pub fn new(
        block_source: ProductReceiver<Vec<u8>>,
        writer: impl 'static + Write + Send,
    ) -> Self {
        let num_bytes_written = Arc::new(0.into());
        let join_handle =
            _launch_worker_thread(block_source, writer, Arc::clone(&num_bytes_written));
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

fn _launch_worker_thread(
    block_source: ProductReceiver<Vec<u8>>,
    mut writer: impl 'static + Write + Send,
    num_bytes_written: Arc<AtomicU64>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            log::debug!("Getting blocks...");
            // TODO Test that crashes bubble up correctly
            let blocks = block_source.get_all_available_products();
            let mut io_slices: Vec<IoSlice> = (&blocks)
                .into_iter()
                .map(|block| IoSlice::new(&block))
                .collect();
            log::debug!("Getting blocks...writing block...");
            let write_result = writer.write_all_vectored(&mut io_slices);
            if let Err(err) = &write_result {
                if err.kind() == ErrorKind::UnexpectedEof {
                    log::debug!("Encountered EOF");
                    return;
                } else {
                    write_result.unwrap();
                }
            }
            let written_size: u64 = blocks
                .into_iter()
                .map(|block| block.len() as u64)
                .sum::<u64>();
            num_bytes_written.fetch_add(written_size, Ordering::Relaxed);
            log::debug!("Getting block...writing block...finished");
        }
    })
}
