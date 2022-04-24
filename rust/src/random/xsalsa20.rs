use anyhow::Result;
use generic_array::{typenum::U56, GenericArray};
use salsa20::{
    cipher::{KeyIvInit, StreamCipher},
    XSalsa20,
};

use super::reseeding::SeedableRandomGenerator;
use crate::byte_stream::SyncByteStream;

/// A block source that outputs a XSalsa20 stream
pub struct XSalsa20Rng {
    // Using stream cipher XSalsa20 because rand::rngs::StdRng says it's not reproducible
    // and even rand_chacha, while reproducible, doesn't return the same result for
    // fill_bytes(5) + fill_bytes(5) than for fill_bytes(10). This is because fill_bytes
    // gets whole words and drops any leftover bytes on each call.
    rng: XSalsa20,
}

impl XSalsa20Rng {
    /// Generate a random generator from a simple u64 seed.
    /// This isn't secure since XSalsa20 has longer seeds than
    /// 8 bytes
    #[cfg(test)]
    pub fn from_u64_seed(seed: u64) -> Self {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        let nonce = [0u8; 24];
        Self {
            rng: XSalsa20::new_from_slices(&key, &nonce).unwrap(),
        }
    }
}

impl SeedableRandomGenerator for XSalsa20Rng {
    // 32 bytes for key and 24 bytes for nonce
    type SeedSize = U56;

    fn from_seed(seed: GenericArray<u8, Self::SeedSize>) -> Self {
        let key = &seed[..32];
        let nonce = &seed[32..];
        Self {
            rng: XSalsa20::new_from_slices(key, nonce).unwrap(),
        }
    }
}

impl SyncByteStream for XSalsa20Rng {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        dest.fill(0);
        self.rng.apply_keystream(dest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_read() {
        // Just making sure it doesn't crash
        let mut data = [0u8; 1024];
        XSalsa20Rng::from_u64_seed(0)
            .blocking_read(&mut data)
            .unwrap();
    }

    #[test]
    fn different_block_sizes_still_sync_read_same_data() {
        // Regression test. We first used rand/rand_chacha, but the rngs in there
        // don't guarantee this and their fill_bytes implementation reads whole words
        // and drops whatever is left over, skipping parts of the stream.
        let mut stream1 = XSalsa20Rng::from_u64_seed(0);
        let mut stream2 = XSalsa20Rng::from_u64_seed(0);
        let mut lhs = [0; 1234];
        let mut rhs = [0; 1234];
        stream1.blocking_read(&mut lhs).unwrap();
        stream2.blocking_read(&mut rhs[..1]).unwrap();
        stream2.blocking_read(&mut rhs[1..10]).unwrap();
        stream2.blocking_read(&mut rhs[10..100]).unwrap();
        stream2.blocking_read(&mut rhs[100..1000]).unwrap();
        stream2.blocking_read(&mut rhs[1000..1234]).unwrap();
        assert_eq!(lhs, rhs);
    }
}
