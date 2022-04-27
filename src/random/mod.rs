use anyhow::Result;

mod composite;
mod os;
mod rdrand;
mod reseeding;
mod xsalsa20;

use crate::byte_stream::{SyncByteStream, XorByteStream};
use reseeding::ReseedingRandomGenerator;

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
