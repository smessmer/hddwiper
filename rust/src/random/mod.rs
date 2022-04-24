use anyhow::Result;

mod composite;
mod os;
mod rdrand;
mod reseeding;
mod xsalsa20;

use crate::byte_stream::SyncByteStream;
use reseeding::ReseedingRandomGenerator;

pub use xsalsa20::XSalsa20Rng;

pub fn secure_seed_rng() -> Result<impl SyncByteStream + Clone> {
    // XOR rdseed and os_rng. So even if RDSEED isn't supported on the platform,
    // os_rng still provides entropy.
    let rdseed = rdrand::RdSeedGenerator::new_if_supported();
    let os_rng = os::OsRandomGenerator;
    Ok(crate::composite_rng!(os_rng, rdseed))
}

pub fn secure_rng(seed_source: impl SyncByteStream + Send) -> Result<impl SyncByteStream> {
    // XOR rdrand and xsalsa20. So even if RDRAND isn't supported on the platform,
    // xsalsa20 still provides entropy.

    // TODO This setup means that one random generator worker thread handles both rdrand and xsalsa.
    //      They could block each other, it might be better to offload them to separate threads.

    const RESEED_EVERY_N_BYTES: usize = 1024 * 1024 * 1024;

    let rdrand = rdrand::RdRandGenerator::new_if_supported();
    // Using both chacha20 and xchacha20 because xchacha20 is our own implementation and in case it's buggy, we also xor in a third party chacha20 implementation
    let xsalsa20 =
        ReseedingRandomGenerator::<XSalsa20Rng, _>::new(RESEED_EVERY_N_BYTES, seed_source);

    Ok(crate::composite_rng!(rdrand, xsalsa20))
}
