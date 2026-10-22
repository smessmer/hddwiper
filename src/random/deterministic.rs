use anyhow::{ensure, Result};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use super::{secure_seed_rng, XSalsa20Rng};
use crate::byte_stream::{SeekableByteStream, SyncByteStream};

/// The seed that all data of a deterministic wipe is derived from.
/// It is displayed as, and parsed from, 64 hex characters.
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl FromStr for Seed {
    type Err = anyhow::Error;

    fn from_str(seed: &str) -> Result<Self> {
        ensure!(
            seed.len() == 64 && seed.bytes().all(|c| c.is_ascii_hexdigit()),
            "A seed consists of 64 hex characters, but '{seed}' doesn't"
        );
        let mut bytes = [0; 32];
        for (byte, hex_digits) in bytes.iter_mut().zip(seed.as_bytes().chunks(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(hex_digits)?, 16)?;
        }
        Ok(Self(bytes))
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
    fn data_of_a_seed_never_changes() {
        // If a seed ever stood for other data, devices that an older version wiped couldn't be
        // verified anymore. The data is XSalsa20 with the seed as its key and a nonce of
        // zeroes. The expected values were computed with libsodium. The second one is at
        // 256 GiB, where XSalsa20's block counter needs more than 32 bit.
        let seed: Seed = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
            .parse()
            .unwrap();
        let mut rng = deterministic_rng(&seed);
        let mut data = [0; 64];

        rng.blocking_read(&mut data).unwrap();
        assert_eq!(
            "a141cb3246c2dd6bfbb8aedfaee2caf0d6c7d5fd72abfca1ee1cbbbd94058ddc\
             4a0ef04198aabf335c54720f2546bed2e43474f047e6284c6b034b8d3cc9df32",
            hex::encode(data)
        );

        rng.seek(256 * 1024 * 1024 * 1024).unwrap();
        rng.blocking_read(&mut data).unwrap();
        assert_eq!(
            "41d14fd961c190526fadfa18bbf6a3ca645faa46d831b9483fee0e47b2915e54\
             e045745c137bb759c2d0f37edfd58c9f2d0acc1ecc0d1ff944aee27f56991f61",
            hex::encode(data)
        );
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
    fn seed_is_parsed_from_hex() {
        let mut seed = [0; 32];
        seed[0] = 0x01;
        seed[1] = 0xab;
        seed[31] = 0xf0;
        let seed = Seed(seed);

        let lowercase = format!("01ab{}f0", "00".repeat(29));
        assert_eq!(seed, lowercase.parse().unwrap());
        assert_eq!(seed, lowercase.to_uppercase().parse().unwrap());
    }

    #[test]
    fn displayed_seed_can_be_parsed_again() {
        let seed = Seed::generate().unwrap();
        assert_eq!(seed, seed.to_string().parse().unwrap());
    }

    #[test]
    fn invalid_seeds_arent_parsed() {
        let valid = "0123456789abcdef".repeat(4);
        assert!(valid.parse::<Seed>().is_ok());

        // Too short and too long
        assert!("".parse::<Seed>().is_err());
        assert!(valid[..62].parse::<Seed>().is_err());
        assert!(valid[..63].parse::<Seed>().is_err());
        assert!(format!("{valid}0").parse::<Seed>().is_err());
        assert!(format!("{valid}00").parse::<Seed>().is_err());

        // Not hex. That includes signs, which a plain number parser would accept.
        assert!(valid.replacen('0', "g", 1).parse::<Seed>().is_err());
        assert!(valid.replacen('0', "+", 1).parse::<Seed>().is_err());
        assert!(valid.replacen('0', " ", 1).parse::<Seed>().is_err());
        assert!(valid.replacen("01", "é", 1).parse::<Seed>().is_err());
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
