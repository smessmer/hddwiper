use anyhow::Result;

use super::SyncByteStream;

/// A byte stream combinator taking two byte streams and outputting
/// the XOR combination of both. Computation of the two wrapped
/// byte streams is put on two different threads.
pub struct XorByteStream<S1: SyncByteStream + Send + 'static, S2: SyncByteStream> {
    stream1: S1,
    stream2: S2,
}

impl<S1: SyncByteStream + Send + 'static, S2: SyncByteStream> XorByteStream<S1, S2> {
    pub fn new(stream1: S1, stream2: S2) -> Self {
        Self { stream1, stream2 }
    }
}

impl<S1: SyncByteStream + Send + 'static, S2: SyncByteStream> SyncByteStream
    for XorByteStream<S1, S2>
{
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        // TODO Avoid zero initializing
        let mut buffer = vec![0; dest.len()];

        // TODO It may make sense to not spawn a new thread for every request but just keep a worker thread around
        std::thread::scope(|s| {
            // Offload stream1 to a different thread
            let thread = s.spawn(|| self.stream1.blocking_read(&mut buffer));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_stream::testutils::FakeByteStream;

    #[test]
    fn xor_of_two_streams() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut result = [0u8; 100];
        xor_stream.blocking_read(&mut result).unwrap();

        // Verify result is XOR of both streams
        let mut expected1 = [0u8; 100];
        let mut expected2 = [0u8; 100];
        FakeByteStream::new(1)
            .blocking_read(&mut expected1)
            .unwrap();
        FakeByteStream::new(2)
            .blocking_read(&mut expected2)
            .unwrap();

        for i in 0..100 {
            assert_eq!(result[i], expected1[i] ^ expected2[i]);
        }
    }

    #[test]
    fn xor_with_itself_produces_zeroes() {
        let stream1 = FakeByteStream::new(42);
        let stream2 = FakeByteStream::new(42);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut result = [0u8; 100];
        xor_stream.blocking_read(&mut result).unwrap();

        // XOR with itself should produce all zeros
        assert!(result.iter().all(|&b| b == 0));
    }

    #[test]
    fn multiple_reads_produce_different_data() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut result1 = [0u8; 100];
        let mut result2 = [0u8; 100];
        xor_stream.blocking_read(&mut result1).unwrap();
        xor_stream.blocking_read(&mut result2).unwrap();

        assert_ne!(result1, result2);
    }

    #[test]
    fn large_reads_work() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut result = vec![0u8; 1024 * 1024]; // 1MB
        xor_stream.blocking_read(&mut result).unwrap();

        // Verify result is XOR of both streams
        let mut expected1 = vec![0u8; 1024 * 1024];
        let mut expected2 = vec![0u8; 1024 * 1024];
        FakeByteStream::new(1)
            .blocking_read(&mut expected1)
            .unwrap();
        FakeByteStream::new(2)
            .blocking_read(&mut expected2)
            .unwrap();

        for i in 0..result.len() {
            assert_eq!(result[i], expected1[i] ^ expected2[i]);
        }
    }

    #[test]
    fn apply_xor_function() {
        let mut dest = [0u8, 1, 2, 3, 4];
        let source = [0u8, 1, 2, 3, 4];
        _apply_xor(&mut dest, &source);
        assert!(dest.iter().all(|&b| b == 0));

        let mut dest2 = [0xFFu8; 5];
        let source2 = [0x00u8; 5];
        _apply_xor(&mut dest2, &source2);
        assert!(dest2.iter().all(|&b| b == 0xFF));
    }
}
