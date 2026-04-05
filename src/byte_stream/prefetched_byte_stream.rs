use std::sync::mpsc;

use super::SyncByteStream;

/// A byte stream wrapper that runs the underlying stream on a dedicated
/// background thread with double buffering.
///
/// On construction, the caller provides an initial buffer and the worker
/// immediately starts filling it. The caller then repeatedly calls
/// [`swap`] with a new buffer, receiving the previously filled buffer
/// in return while the worker begins filling the new one.
///
/// This ensures the worker is always one step ahead, hiding its latency
/// behind whatever work the caller does between swaps.
pub struct PrefetchedByteStream {
    request_tx: mpsc::SyncSender<Vec<u8>>,
    response_rx: mpsc::Receiver<Vec<u8>>,
    _join_handle: std::thread::JoinHandle<()>,
}

impl PrefetchedByteStream {
    /// Spawn a background thread that fills buffers using `stream`.
    /// Immediately starts filling `initial_buffer`.
    pub fn new(stream: impl SyncByteStream + Send + 'static, initial_buffer: Vec<u8>) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<Vec<u8>>(0);
        let (response_tx, response_rx) = mpsc::sync_channel::<Vec<u8>>(0);

        let join_handle = std::thread::spawn(move || {
            _worker_thread(stream, request_rx, response_tx);
        });

        // Kick off the first fill immediately
        request_tx.send(initial_buffer).unwrap();

        Self {
            request_tx,
            response_rx,
            _join_handle: join_handle,
        }
    }

    /// Hand in `next_buffer` for the worker to fill, and receive the
    /// previously filled buffer. The worker starts filling `next_buffer`
    /// immediately, so the caller can process the returned data while
    /// the next batch is being prepared.
    pub fn swap(&self, next_buffer: Vec<u8>) -> Vec<u8> {
        let filled = self.response_rx.recv().unwrap();
        self.request_tx.send(next_buffer).unwrap();
        filled
    }
}

fn _worker_thread(
    mut stream: impl SyncByteStream,
    request_rx: mpsc::Receiver<Vec<u8>>,
    response_tx: mpsc::SyncSender<Vec<u8>>,
) {
    while let Ok(mut buffer) = request_rx.recv() {
        stream.blocking_read(&mut buffer).unwrap();
        if response_tx.send(buffer).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_stream::testutils::FakeByteStream;

    #[test]
    fn returns_filled_data() {
        let stream = FakeByteStream::new(42);
        let prefetched = PrefetchedByteStream::new(stream, vec![0u8; 100]);

        let filled = prefetched.swap(vec![0u8; 100]);
        assert_eq!(filled.len(), 100);
        assert!(filled.iter().any(|&b| b != 0));
    }

    #[test]
    fn multiple_swaps_produce_different_data() {
        let stream = FakeByteStream::new(1);
        let prefetched = PrefetchedByteStream::new(stream, vec![0u8; 100]);

        let first = prefetched.swap(vec![0u8; 100]);
        let second = prefetched.swap(vec![0u8; 100]);
        assert_ne!(first, second);
    }

    #[test]
    fn buffers_are_reused_via_swap() {
        let stream = FakeByteStream::new(1);
        let prefetched = PrefetchedByteStream::new(stream, vec![0u8; 100]);

        let buf_a = prefetched.swap(vec![0u8; 100]);
        assert_eq!(buf_a.len(), 100);
        let buf_b = prefetched.swap(buf_a);
        assert_eq!(buf_b.len(), 100);
    }

    #[test]
    fn produces_same_data_as_direct_stream() {
        let mut direct = FakeByteStream::new(7);
        let prefetched = PrefetchedByteStream::new(FakeByteStream::new(7), vec![0u8; 256]);

        let mut expected = vec![0u8; 256];
        direct.blocking_read(&mut expected).unwrap();

        let actual = prefetched.swap(vec![0u8; 256]);
        assert_eq!(actual, expected);
    }
}
