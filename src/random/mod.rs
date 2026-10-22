use anyhow::Result;

mod composite;
mod deterministic;
mod os;
mod rdrand;
mod reseeding;
mod xsalsa20;

use crate::byte_stream::{SyncByteStream, XorByteStream};
use reseeding::ReseedingRandomGenerator;

pub use deterministic::{deterministic_rng, Seed};
pub use xsalsa20::XSalsa20Rng;

pub fn secure_seed_rng() -> Result<impl SyncByteStream + Clone> {
    // XOR rdseed and os_rng. So even if RDSEED isn't supported on the platform,
    // os_rng still provides entropy.
    let rdseed = rdrand::RdSeedGenerator::new_if_supported();
    let os_rng = os::OsRandomGenerator;
    Ok(crate::composite_rng!(os_rng, rdseed))
}

pub fn secure_rng(
    seed_source: impl SyncByteStream + Send,
    disable_rdrand: bool,
) -> impl SyncByteStream {
    let rng_rdrand = if disable_rdrand {
        rdrand::RdRandGenerator::new_zeroes()
    } else {
        rdrand::RdRandGenerator::new_if_supported()
    };

    const RESEED_EVERY_N_BYTES: usize = 1024 * 1024 * 1024;
    let rng_xsalsa =
        ReseedingRandomGenerator::<XSalsa20Rng, _>::new(RESEED_EVERY_N_BYTES, seed_source);

    XorByteStream::new(rng_rdrand, rng_xsalsa)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_stream::testutils::FakeByteStream;
    use generic_array::GenericArray;
    use reseeding::SeedableRandomGenerator;

    fn read(mut rng: impl SyncByteStream, num_bytes: usize) -> Vec<u8> {
        let mut data = vec![0; num_bytes];
        rng.blocking_read(&mut data).unwrap();
        data
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn cpu_has_rdrand() -> bool {
        ::rdrand::RdRand::new().is_ok()
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    fn cpu_has_rdrand() -> bool {
        false
    }

    #[test]
    fn secure_rng_without_rdrand_is_xsalsa20_seeded_from_the_seed_source() {
        // The seed is the first thing that is taken from the seed source
        let mut seed = GenericArray::default();
        FakeByteStream::new(1).blocking_read(&mut seed).unwrap();
        let expected = read(XSalsa20Rng::from_seed(seed), 10_000);

        assert!(expected == read(secure_rng(FakeByteStream::new(1), true), 10_000));
    }

    #[test]
    fn secure_rng_with_another_seed_generates_other_data() {
        assert!(
            read(secure_rng(FakeByteStream::new(1), true), 10_000)
                != read(secure_rng(FakeByteStream::new(2), true), 10_000)
        );
    }

    #[test]
    fn secure_rng_mixes_in_rdrand() {
        // Both have the same seed, so they can only differ in what RDRAND adds
        let data1 = read(secure_rng(FakeByteStream::new(1), false), 10_000);
        let data2 = read(secure_rng(FakeByteStream::new(1), false), 10_000);

        if cpu_has_rdrand() {
            assert!(data1 != data2);
            // And neither of them is what it would be without RDRAND
            assert!(data1 != read(secure_rng(FakeByteStream::new(1), true), 10_000));
        } else {
            assert!(data1 == data2);
        }
    }

    #[test]
    fn secure_seed_rng_generates_different_data_each_time() {
        let data1 = read(secure_seed_rng().unwrap(), 100);
        let data2 = read(secure_seed_rng().unwrap(), 100);
        assert_ne!(data1, data2);
        assert_ne!(vec![0; 100], data1);
    }

    #[test]
    fn clones_of_secure_seed_rng_generate_different_data() {
        // Each worker that needs seeds gets a clone, and they must not all get the same seeds
        let rng = secure_seed_rng().unwrap();
        assert_ne!(read(rng.clone(), 100), read(rng, 100));
    }
}
