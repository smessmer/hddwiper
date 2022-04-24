use c2_chacha::{stream_cipher::{StreamCipher, NewStreamCipher}, XChaCha20};
use rand::SeedableRng;
use rand_core::block::BlockRngCore;
use zerocopy::AsBytes;

pub struct XChaCha20Rng {
    seed: <Self as SeedableRng>::Seed,
    cipher: XChaCha20,
}

impl XChaCha20Rng {
    pub fn new(seed: <Self as SeedableRng>::Seed) -> Self {
        let seed_ref: &[u8] = seed.as_ref();
        let key = &seed_ref[..32];
        let nonce: &[u8]=&seed_ref[32..];
        let cipher = XChaCha20::new_var(key, nonce).unwrap();
        Self {
            seed,
            cipher,
        }
    }
}

impl Clone for XChaCha20Rng {
    fn clone(&self) -> Self {
        Self::new(self.seed.clone())
    }
}

impl BlockRngCore for XChaCha20Rng {
    type Item = u32;
    type Results = [u32; 64 / std::mem::size_of::<u32>()]; // 64 is XChaCha20 block size
    fn generate(&mut self, results: &mut Self::Results) {
        results.as_mut().fill(0);
        let data = results.as_bytes_mut();
        self.cipher.encrypt(data);
    }
}

#[derive(Clone)]
pub struct Array<const N: usize> {
    seed: [u8; N],
}

impl <const N: usize> Default for Array<N> {
    fn default() -> Self {
        Self {
            seed: [0; N],
        }
    }
}

impl <const N: usize> AsRef<[u8]> for Array<N> {
    fn as_ref(&self) -> &[u8] {
        self.seed.as_bytes()
    }
}

impl <const N: usize> AsMut<[u8]> for Array<N> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.seed.as_bytes_mut()
    }
}

// 32 bytes for key, 24 bytes for nonce
pub type XChaCha20Seed = Array<{32 + 24}>;

impl SeedableRng for XChaCha20Rng {
    type Seed = XChaCha20Seed;

    fn from_seed(seed: Self::Seed) -> Self {
        Self::new(seed)
    }
}
