use anyhow::Result;

use super::SyncByteStream;

/// A byte stream combinator taking two byte streams and outputting
/// the XOR combination of both. Computation of the two wrapped
/// byte streams is put on two different threads.
pub struct XorByteStream<S1: SyncByteStream + Send + 'static, S2: SyncByteStream> {
    stream1: S1,
    stream2: S2,
}

impl <S1: SyncByteStream + Send + 'static, S2: SyncByteStream> XorByteStream<S1, S2> {
    pub fn new(stream1: S1, stream2: S2) -> Self {
        Self {
            stream1,
            stream2,
        }
    }
}

impl <S1: SyncByteStream + Send + 'static, S2: SyncByteStream> SyncByteStream for XorByteStream<S1, S2> {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        // TODO Avoid zero initializing
        let mut buffer = vec![0; dest.len()];

        // TODO It may make sense to not spawn a new thread for every request but just keep a worker thread around
        std::thread::scope(|s| {
            // Offload stream1 to a different thread
            let thread = s.spawn(|| {
                self.stream1.blocking_read(&mut buffer)
            });
            // Calculate stream2 on the current thread
            self.stream2.blocking_read(dest).unwrap();
            thread.join().unwrap().unwrap();
        });

        _apply_xor(dest, &buffer);

        Ok(())
    }
}

fn _apply_xor(dest: &mut [u8], source: &[u8]) {
    assert_eq!(dest.len(), source.len());
    for i in 0..dest.len() {
        dest[i] ^= source[i];
    }
}
