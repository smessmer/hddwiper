use anyhow::Result;
use std::fmt::{self, Display, Formatter};

use super::{secure_seed_rng, XSalsa20Rng};
use crate::byte_stream::{SeekableByteStream, SyncByteStream};

/// The seed that all data of a deterministic wipe is derived from.
/// It is displayed as 64 hex characters.
pub struct Seed([u8; 32]);

impl Seed {
    /// Generate a new random seed
    pub fn generate() -> Result<Self> {
        let mut seed = [0; 32];
        secure_seed_rng()?.blocking_read(&mut seed)?;
        Ok(Self(seed))
    }
}

impl Display for Seed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A random generator whose output only depends on `seed` and on the position in its stream.
/// Unlike [super::secure_rng], it never reseeds and doesn't mix in RDRAND, because then
/// the same data couldn't be generated again.
pub fn deterministic_rng(seed: &Seed) -> impl SeekableByteStream {
    XSalsa20Rng::from_key(&seed.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(mut rng: impl SyncByteStream) -> [u8; 1234] {
        let mut data = [0; 1234];
        rng.blocking_read(&mut data).unwrap();
        data
    }

    #[test]
    fn seed_is_displayed_as_hex() {
        let mut seed = [0; 32];
        seed[0] = 0x01;
        seed[1] = 0xab;
        seed[31] = 0xf0;
        assert_eq!(format!("01ab{}f0", "00".repeat(29)), Seed(seed).to_string());
    }

    #[test]
    fn generated_seeds_are_different() {
        let seed1 = Seed::generate().unwrap();
        let seed2 = Seed::generate().unwrap();
        assert_ne!(seed1.0, seed2.0);
    }

    #[test]
    fn same_seed_generates_same_data() {
        let seed = Seed::generate().unwrap();
        assert_eq!(
            read(deterministic_rng(&seed)),
            read(deterministic_rng(&seed))
        );
    }

    #[test]
    fn different_seeds_generate_different_data() {
        let seed1 = Seed::generate().unwrap();
        let seed2 = Seed::generate().unwrap();
        assert_ne!(
            read(deterministic_rng(&seed1)),
            read(deterministic_rng(&seed2))
        );
    }
}
