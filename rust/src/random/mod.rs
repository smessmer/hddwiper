use anyhow::Result;

mod composite;
mod os;
mod rdrand;
mod reseeding;
mod xsalsa20;

use crate::byte_stream::SyncByteStream;
use reseeding::ReseedingRandomGenerator;

pub use self::rdrand::SyncByteStreamOrZeroes;
pub use xsalsa20::XSalsa20Rng;

pub fn secure_seed_rng() -> Result<impl SyncByteStream + Clone> {
    // XOR rdseed and os_rng. So even if RDSEED isn't supported on the platform,
    // os_rng still provides entropy.
    let rdseed = rdrand::RdSeedGenerator::new_if_supported();
    let os_rng = os::OsRandomGenerator;
    Ok(crate::composite_rng!(os_rng, rdseed))
}

pub fn rng_xsalsa(seed_source: impl SyncByteStream + Send) -> impl SyncByteStream {
    const RESEED_EVERY_N_BYTES: usize = 1024 * 1024 * 1024;
    ReseedingRandomGenerator::<XSalsa20Rng, _>::new(RESEED_EVERY_N_BYTES, seed_source)
}

pub fn rng_rdrand_or_zeroes() -> SyncByteStreamOrZeroes<rdrand::RdRandGenerator> {
    rdrand::RdRandGenerator::new_if_supported()
}

pub fn rng_zeroes() -> SyncByteStreamOrZeroes<rdrand::RdRandGenerator> {
    rdrand::SyncByteStreamOrZeroes::<rdrand::RdRandGenerator>::new_zeroes()
}
