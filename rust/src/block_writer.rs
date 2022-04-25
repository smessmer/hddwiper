use std::thread::{self, JoinHandle};
use std::io::{ErrorKind, Write};
use std::sync::{Arc, atomic::{Ordering, AtomicU64}};

use crate::byte_stream::SyncBlockSource;

/// Launches a thread and writes everything from a [BlockSource] into a [Writer], e.g. a [std::fs::File].
pub struct BlockWriter {
    join_handle: JoinHandle<()>,
    num_bytes_written: Arc<AtomicU64>,
}

impl BlockWriter {
    /// Launch a new thread that writes blocks from `block_source` into `writer` until the `writer` is EOF
    /// or the [BlockWriter] gets cancelled
    pub fn new(block_source: impl 'static + SyncBlockSource + Send, writer: impl 'static + Write + Send) -> Self {
        let num_bytes_written = Arc::new(0.into());
        let join_handle = _launch_worker_thread(block_source, writer, Arc::clone(&num_bytes_written));
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

fn _launch_worker_thread(mut block_source: impl 'static + SyncBlockSource + Send, mut writer: impl 'static + Write + Send, num_bytes_written: Arc<AtomicU64>) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            // TODO Test that crashes bubble up correctly
            // TODO use timeout instead of blocking indefinitely so that we can cancel faster
            let product = block_source.blocking_read().unwrap();
            let write_result = writer.write_all(&product) ;
            if let Err(err) = &write_result {
                if err.kind() == ErrorKind::UnexpectedEof {
                    log::debug!("Encountered EOF");
                    return;
                } else {
                    write_result.unwrap();
                }
            }
            num_bytes_written.fetch_add(product.len() as u64, Ordering::Relaxed);
        }
    })
}
