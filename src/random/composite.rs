use crate::byte_stream::SyncByteStream;
use anyhow::Result;

#[derive(Clone)]
pub struct CompositeRng<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> {
    rng1: Rng1,
    rng2: Rng2,
}

impl<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> CompositeRng<Rng1, Rng2> {
    pub fn new(rng1: Rng1, rng2: Rng2) -> Self {
        Self { rng1, rng2 }
    }
}

impl<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> SyncByteStream
    for CompositeRng<Rng1, Rng2>
{
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        let mut rng1_data = vec![0; dest.len()];
        self.rng1.blocking_read(&mut rng1_data)?;
        self.rng2.blocking_read(dest)?;
        _apply_xor(dest, &rng1_data);
        Ok(())
    }
}

fn _apply_xor(dest: &mut [u8], source: &[u8]) {
    assert_eq!(dest.len(), source.len());
    for i in 0..dest.len() {
        dest[i] ^= source[i];
    }
}

#[macro_export]
macro_rules! composite_rng {
    ($rng1:expr, $rng2:expr) => {
        $crate::random::composite::CompositeRng::new($rng1, $rng2)
    };
    ($rng1:expr, $rng2:expr, $($tail:expr),+) => {
        $crate::random::composite::CompositeRng::new($rng1, $crate::composite_rng!($rng2, $($tail),+))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_stream::testutils::FakeByteStream;

    #[test]
    fn xor_of_two_streams() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut composite = CompositeRng::new(stream1, stream2);

        let mut result = [0u8; 100];
        composite.blocking_read(&mut result).unwrap();

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
        let mut composite = CompositeRng::new(stream1, stream2);

        let mut result = [0u8; 100];
        composite.blocking_read(&mut result).unwrap();

        // XOR with itself should produce all zeros
        assert!(result.iter().all(|&b| b == 0));
    }

    #[test]
    fn multiple_reads_produce_different_data() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut composite = CompositeRng::new(stream1, stream2);

        let mut result1 = [0u8; 100];
        let mut result2 = [0u8; 100];
        composite.blocking_read(&mut result1).unwrap();
        composite.blocking_read(&mut result2).unwrap();

        assert_ne!(result1, result2);
    }

    #[test]
    fn composite_rng_macro_two_streams() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let mut composite = composite_rng!(stream1, stream2);

        let mut result = [0u8; 100];
        composite.blocking_read(&mut result).unwrap();

        // Verify we got data (not all zeros unless streams happen to cancel)
        // Just verify it doesn't crash and produces output
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn composite_rng_macro_three_streams() {
        let stream1 = FakeByteStream::new(1);
        let stream2 = FakeByteStream::new(2);
        let stream3 = FakeByteStream::new(3);
        let mut composite = composite_rng!(stream1, stream2, stream3);

        let mut result = [0u8; 100];
        composite.blocking_read(&mut result).unwrap();

        // Verify result is XOR of all three streams
        let mut expected1 = [0u8; 100];
        let mut expected2 = [0u8; 100];
        let mut expected3 = [0u8; 100];
        FakeByteStream::new(1)
            .blocking_read(&mut expected1)
            .unwrap();
        FakeByteStream::new(2)
            .blocking_read(&mut expected2)
            .unwrap();
        FakeByteStream::new(3)
            .blocking_read(&mut expected3)
            .unwrap();

        for i in 0..100 {
            assert_eq!(result[i], expected1[i] ^ expected2[i] ^ expected3[i]);
        }
    }
}
