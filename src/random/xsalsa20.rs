use anyhow::{anyhow, Result};
use generic_array::{typenum::U56, GenericArray};
use salsa20::{
    cipher::{KeyIvInit, StreamCipher, StreamCipherSeek},
    XSalsa20,
};

use super::reseeding::SeedableRandomGenerator;
use crate::byte_stream::{SeekableByteStream, SyncByteStream};

/// A block source that outputs a XSalsa20 stream
pub struct XSalsa20Rng {
    // Using stream cipher XSalsa20 because rand::rngs::StdRng says it's not reproducible
    // and even rand_chacha, while reproducible, doesn't return the same result for
    // fill_bytes(5) + fill_bytes(5) than for fill_bytes(10). This is because fill_bytes
    // gets whole words and drops any leftover bytes on each call.
    rng: XSalsa20,
}

impl XSalsa20Rng {
    /// Generate a random generator from a key alone. The nonce is always the same,
    /// so the key must not be used for anything else.
    pub fn from_key(key: &[u8; 32]) -> Self {
        let nonce = [0u8; 24];
        Self {
            rng: XSalsa20::new_from_slices(key, &nonce).unwrap(),
        }
    }

    /// Generate a random generator from a simple u64 seed.
    /// This isn't secure since XSalsa20 has longer seeds than
    /// 8 bytes
    #[cfg(test)]
    pub fn from_u64_seed(seed: u64) -> Self {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        Self::from_key(&key)
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

impl SeekableByteStream for XSalsa20Rng {
    fn seek(&mut self, position: u64) -> Result<()> {
        // XSalsa20 computes each of its 64 byte blocks from the key and the number of the block,
        // so this is cheap and doesn't have to generate anything before `position`.
        self.rng
            .try_seek(position)
            .map_err(|_| anyhow!("Position {position} is beyond the end of the XSalsa20 stream"))
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

    #[test]
    fn same_key_generates_same_data() {
        let mut lhs = [0; 1234];
        let mut rhs = [0; 1234];
        XSalsa20Rng::from_key(&[42; 32])
            .blocking_read(&mut lhs)
            .unwrap();
        XSalsa20Rng::from_key(&[42; 32])
            .blocking_read(&mut rhs)
            .unwrap();
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn different_keys_generate_different_data() {
        let mut lhs = [0; 1234];
        let mut rhs = [0; 1234];
        XSalsa20Rng::from_key(&[42; 32])
            .blocking_read(&mut lhs)
            .unwrap();
        XSalsa20Rng::from_key(&[43; 32])
            .blocking_read(&mut rhs)
            .unwrap();
        assert_ne!(lhs, rhs);
    }

    fn read(mut rng: XSalsa20Rng) -> [u8; 1234] {
        let mut data = [0; 1234];
        rng.blocking_read(&mut data).unwrap();
        data
    }

    fn seed(key: [u8; 32], nonce: [u8; 24]) -> GenericArray<u8, U56> {
        let mut seed = GenericArray::default();
        seed[..32].copy_from_slice(&key);
        seed[32..].copy_from_slice(&nonce);
        seed
    }

    #[test]
    fn seed_consists_of_key_and_nonce() {
        // from_key() is the same with a nonce of zeroes
        assert_eq!(
            read(XSalsa20Rng::from_key(&[42; 32])),
            read(XSalsa20Rng::from_seed(seed([42; 32], [0; 24])))
        );
    }

    #[test]
    fn same_seed_generates_same_data() {
        assert_eq!(
            read(XSalsa20Rng::from_seed(seed([42; 32], [7; 24]))),
            read(XSalsa20Rng::from_seed(seed([42; 32], [7; 24])))
        );
    }

    #[test]
    fn different_seeds_generate_different_data() {
        let data = read(XSalsa20Rng::from_seed(seed([42; 32], [7; 24])));

        // The key and the nonce both matter, up to their last byte
        let mut other_key = [42; 32];
        other_key[31] = 43;
        assert_ne!(data, read(XSalsa20Rng::from_seed(seed(other_key, [7; 24]))));
        let mut other_nonce = [7; 24];
        other_nonce[23] = 8;
        assert_ne!(
            data,
            read(XSalsa20Rng::from_seed(seed([42; 32], other_nonce)))
        );
    }

    #[test]
    fn seek_continues_stream_at_position() {
        let mut expected = [0; 1000];
        XSalsa20Rng::from_u64_seed(0)
            .blocking_read(&mut expected)
            .unwrap();

        // Seek forwards and backwards, to positions on and next to the borders of XSalsa20's 64 byte blocks
        let mut stream = XSalsa20Rng::from_u64_seed(0);
        for position in [500, 0, 63, 64, 65, 1, 999, 128] {
            let mut actual = vec![0; expected.len() - position];
            stream.seek(position as u64).unwrap();
            stream.blocking_read(&mut actual).unwrap();
            assert_eq!(
                hex::encode(&expected[position..]),
                hex::encode(actual),
                "Wrong data after seeking to position {position}"
            );
        }
    }

    #[test]
    fn stream_doesnt_repeat_after_256_gib() {
        // XSalsa20 numbers its 64 byte blocks. If it did that with 32 bit, the numbers would
        // start over at 256 GiB and so would the stream.
        const POSITION: u64 = (1 << 32) * 64;
        let mut stream = XSalsa20Rng::from_u64_seed(0);

        let mut across_position = [0; 128];
        stream.seek(POSITION - 64).unwrap();
        stream.blocking_read(&mut across_position).unwrap();

        let mut at_position = [0; 64];
        stream.seek(POSITION).unwrap();
        stream.blocking_read(&mut at_position).unwrap();

        let mut at_start = [0; 64];
        stream.seek(0).unwrap();
        stream.blocking_read(&mut at_start).unwrap();

        // Reading on past the position gives the same data as seeking there
        assert_eq!(across_position[64..], at_position);
        assert_ne!(at_start, at_position);
    }
}
