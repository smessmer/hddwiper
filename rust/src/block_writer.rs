use std::io::{ErrorKind, Write};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

use crate::byte_stream::SyncByteStream;

/// Launches a thread and writes everything from a [BlockSource] into a [Writer], e.g. a [std::fs::File].
pub struct BlockWriter {
    join_handle: JoinHandle<()>,
    num_bytes_written: Arc<AtomicU64>,
}

impl BlockWriter {
    /// Launch a new thread that writes blocks from `byte_stream` into `writer` until the `writer` is EOF
    /// or the [BlockWriter] gets cancelled
    pub fn new(
        byte_stream: impl 'static + SyncByteStream + Send,
        block_size: usize,
        writer: impl 'static + Write + Send,
    ) -> Self {
        let num_bytes_written = Arc::new(0.into());
        let join_handle = _launch_worker_thread(
            byte_stream,
            block_size,
            writer,
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

fn _launch_worker_thread(
    mut byte_stream: impl 'static + SyncByteStream + Send,
    block_size: usize,
    mut writer: impl 'static + Write + Send,
    num_bytes_written: Arc<AtomicU64>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        loop {
            log::debug!("Getting block...");
            // TODO Test that crashes bubble up correctly
            // TODO use timeout instead of blocking indefinitely so that we can cancel faster
            let mut block = vec![0u8; block_size];
            byte_stream.blocking_read(&mut block).unwrap();
            log::debug!("Getting block...writing block...");
            let write_result = writer.write_all(&block);
            if let Err(err) = &write_result {
                if err.kind() == ErrorKind::UnexpectedEof {
                    log::debug!("Encountered EOF");
                    return;
                } else {
                    write_result.unwrap();
                }
            }
            num_bytes_written.fetch_add(block_size as u64, Ordering::Relaxed);
            log::debug!("Getting block...writing block...finished");
        }
    })
}
