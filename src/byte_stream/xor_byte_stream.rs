use anyhow::Result;
use std::collections::VecDeque;

use super::prefetched_byte_stream::PrefetchedByteStream;
use super::SyncByteStream;

/// A byte stream combinator taking two byte streams and outputting
/// the XOR combination of both. stream1 runs on a dedicated background
/// thread with double-buffered prefetching to overlap with stream2.
pub struct XorByteStream<S2: SyncByteStream> {
    stream2: S2,
    /// Holds stream1 until the first blocking_read, when we know the buffer size.
    pending_stream1: Option<Box<dyn SyncByteStream + Send>>,
    /// Initialized on first blocking_read with the correct buffer size.
    prefetched: Option<PrefetchedByteStream>,
    /// Spare buffer for the next swap, reused across calls.
    spare_buffer: Option<Vec<u8>>,
    /// The buffer size used for the in-flight prefetch request.
    prefetch_len: usize,
    /// Leftover bytes from a previous prefetch that was larger than needed.
    leftover: VecDeque<u8>,
}

impl<S2: SyncByteStream> XorByteStream<S2> {
    pub fn new(stream1: impl SyncByteStream + Send + 'static, stream2: S2) -> Self {
        Self {
            stream2,
            pending_stream1: Some(Box::new(stream1)),
            prefetched: None,
            spare_buffer: None,
            prefetch_len: 0,
            leftover: VecDeque::new(),
        }
    }
}

impl<S2: SyncByteStream> SyncByteStream for XorByteStream<S2> {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        // Lazily initialize prefetching on the first call
        let prefetched = match &self.prefetched {
            Some(_) => self.prefetched.as_ref().unwrap(),
            None => {
                let stream1 = self.pending_stream1.take().unwrap();
                self.prefetch_len = dest.len();
                self.prefetched
                    .insert(PrefetchedByteStream::new(stream1, vec![0u8; dest.len()]))
            }
        };

        // Compute stream2 into dest while the worker fills its buffer
        self.stream2.blocking_read(dest)?;

        // Collect enough stream1 bytes for XOR, starting with any leftover
        // from a previous oversized prefetch.
        let needed = dest.len();
        let mut stream1_bytes: Vec<u8> = self.leftover.drain(..self.leftover.len().min(needed)).collect();

        while stream1_bytes.len() < needed {
            let spare = self
                .spare_buffer
                .take()
                .unwrap_or_else(|| vec![0u8; self.prefetch_len]);
            let filled = prefetched.swap(spare);

            let take = (needed - stream1_bytes.len()).min(filled.len());
            stream1_bytes.extend_from_slice(&filled[..take]);

            // Save any excess as leftover
            if take < filled.len() {
                self.leftover.extend(&filled[take..]);
            }

            self.spare_buffer = Some(filled);
        }

        // Update prefetch size for next round if it changed
        if self.prefetch_len != dest.len() {
            log::warn!("XorByteStream: read size changed from {} to {} bytes", self.prefetch_len, dest.len());
            self.prefetch_len = dest.len();
        }

        _apply_xor(dest, &stream1_bytes);

        Ok(())
    }
}

fn _apply_xor(dest: &mut [u8], source: &[u8]) {
    assert_eq!(dest.len(), source.len());
    for (d, s) in dest.iter_mut().zip(source.iter()) {
        *d ^= s;
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
    fn changing_read_size_produces_correct_xor() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        // First read with one size
        let mut result1 = vec![0u8; 100];
        xor_stream.blocking_read(&mut result1).unwrap();

        // Second read with a different size
        let mut result2 = vec![0u8; 200];
        xor_stream.blocking_read(&mut result2).unwrap();

        // Verify both reads match the expected XOR output
        let mut s1 = FakeByteStream::new(1);
        let mut s2 = FakeByteStream::new(2);

        let mut expected1_a = vec![0u8; 100];
        let mut expected1_b = vec![0u8; 100];
        s1.blocking_read(&mut expected1_a).unwrap();
        s2.blocking_read(&mut expected1_b).unwrap();
        for i in 0..100 {
            assert_eq!(result1[i], expected1_a[i] ^ expected1_b[i]);
        }

        let mut expected2_a = vec![0u8; 200];
        let mut expected2_b = vec![0u8; 200];
        s1.blocking_read(&mut expected2_a).unwrap();
        s2.blocking_read(&mut expected2_b).unwrap();
        for i in 0..200 {
            assert_eq!(result2[i], expected2_a[i] ^ expected2_b[i]);
        }
    }

    #[test]
    fn changing_read_size_back_and_forth() {
        let stream1 = FakeByteStream::new(3);
        let stream2 = FakeByteStream::new(4);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut s1 = FakeByteStream::new(3);
        let mut s2 = FakeByteStream::new(4);

        for size in [50, 150, 50, 200, 100] {
            let mut result = vec![0u8; size];
            xor_stream.blocking_read(&mut result).unwrap();

            let mut e1 = vec![0u8; size];
            let mut e2 = vec![0u8; size];
            s1.blocking_read(&mut e1).unwrap();
            s2.blocking_read(&mut e2).unwrap();

            for i in 0..size {
                assert_eq!(
                    result[i],
                    e1[i] ^ e2[i],
                    "mismatch at index {} for size {}",
                    i,
                    size
                );
            }
        }
    }

    #[test]
    fn large_to_small_leftovers_span_multiple_reads() {
        // Start with a large read (1000 bytes), then do many small reads (10 bytes each).
        // The prefetch buffer from the first round (1000 bytes) should produce leftovers
        // that satisfy many subsequent small reads without needing new swaps.
        let stream1 = FakeByteStream::new(5);
        let stream2 = FakeByteStream::new(6);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut s1 = FakeByteStream::new(5);
        let mut s2 = FakeByteStream::new(6);

        // First read: large
        let mut result = vec![0u8; 1000];
        xor_stream.blocking_read(&mut result).unwrap();
        let mut e1 = vec![0u8; 1000];
        let mut e2 = vec![0u8; 1000];
        s1.blocking_read(&mut e1).unwrap();
        s2.blocking_read(&mut e2).unwrap();
        for i in 0..1000 {
            assert_eq!(result[i], e1[i] ^ e2[i], "mismatch at index {} in initial large read", i);
        }

        // Many small reads: each 10 bytes, served from leftovers of the 1000-byte prefetch
        for round in 0..200 {
            let mut result = vec![0u8; 10];
            xor_stream.blocking_read(&mut result).unwrap();
            let mut e1 = vec![0u8; 10];
            let mut e2 = vec![0u8; 10];
            s1.blocking_read(&mut e1).unwrap();
            s2.blocking_read(&mut e2).unwrap();
            for i in 0..10 {
                assert_eq!(
                    result[i],
                    e1[i] ^ e2[i],
                    "mismatch at index {} in small read round {}",
                    i,
                    round
                );
            }
        }
    }

    #[test]
    fn single_byte_reads_after_large_prefetch() {
        // Extreme case: 1-byte reads after a 500-byte initial read
        let stream1 = FakeByteStream::new(7);
        let stream2 = FakeByteStream::new(8);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut s1 = FakeByteStream::new(7);
        let mut s2 = FakeByteStream::new(8);

        let mut result = vec![0u8; 500];
        xor_stream.blocking_read(&mut result).unwrap();
        let mut e1 = vec![0u8; 500];
        let mut e2 = vec![0u8; 500];
        s1.blocking_read(&mut e1).unwrap();
        s2.blocking_read(&mut e2).unwrap();
        for i in 0..500 {
            assert_eq!(result[i], e1[i] ^ e2[i]);
        }

        for round in 0..1000 {
            let mut result = [0u8; 1];
            xor_stream.blocking_read(&mut result).unwrap();
            let mut e1 = [0u8; 1];
            let mut e2 = [0u8; 1];
            s1.blocking_read(&mut e1).unwrap();
            s2.blocking_read(&mut e2).unwrap();
            assert_eq!(
                result[0],
                e1[0] ^ e2[0],
                "mismatch in single-byte read round {}",
                round
            );
        }
    }

    #[test]
    fn small_to_large_needs_multiple_swaps() {
        // Start with small reads (10 bytes) then request a large read (5000 bytes)
        // which needs multiple prefetch swaps (each returning 10 bytes) to fill.
        let stream1 = FakeByteStream::new(9);
        let stream2 = FakeByteStream::new(10);
        let mut xor_stream = XorByteStream::new(stream1, stream2);

        let mut s1 = FakeByteStream::new(9);
        let mut s2 = FakeByteStream::new(10);

        // Small reads first to set prefetch_len to 10
        for _ in 0..5 {
            let mut result = vec![0u8; 10];
            xor_stream.blocking_read(&mut result).unwrap();
            let mut e1 = vec![0u8; 10];
            let mut e2 = vec![0u8; 10];
            s1.blocking_read(&mut e1).unwrap();
            s2.blocking_read(&mut e2).unwrap();
            for i in 0..10 {
                assert_eq!(result[i], e1[i] ^ e2[i]);
            }
        }

        // Large read: needs many swaps of 10-byte buffers to fill 5000 bytes
        let mut result = vec![0u8; 5000];
        xor_stream.blocking_read(&mut result).unwrap();
        let mut e1 = vec![0u8; 5000];
        let mut e2 = vec![0u8; 5000];
        s1.blocking_read(&mut e1).unwrap();
        s2.blocking_read(&mut e2).unwrap();
        for i in 0..5000 {
            assert_eq!(result[i], e1[i] ^ e2[i], "mismatch at index {} in large read", i);
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
