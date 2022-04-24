use anyhow::Result;
use rand::rngs::{adapter::ReseedingRng, OsRng};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Core;
use rand_hc::Hc128Core;

mod composite;
mod rdrand;
mod xchacha;

use xchacha::XChaCha20Rng;

pub fn secure_rng() -> Result<impl Rng + Clone> {
    // XOR together a couple different random generators.
    // This is not strictly necessary since most of those generators
    // should be secure by itself, but xoring it with others never hurts
    // for additional security. XORing a good random generator with
    // a bad one yields a random generator that is at least as good
    // as the good one.
    // This approach of xoring together increases the demand on
    // hardware entropy (all of those random generators have to be seeded),
    // so we shouldn't seed too often

    const RESEED_THRESHOLD: u64 = 1024 * 1024 * 1024;

    let rdrand = rdrand::rdrand_or_zeroes();
    // Using both chacha20 and xchacha20 because xchacha20 is our own implementation and in case it's buggy, we also xor in a third party chacha20 implementation
    let chacha = ReseedingRng::new(ChaCha20Core::from_rng(OsRng)?, RESEED_THRESHOLD, OsRng);
    let xchacha = ReseedingRng::new(XChaCha20Rng::from_rng(OsRng)?, RESEED_THRESHOLD, OsRng);
    let hc = ReseedingRng::new(Hc128Core::from_rng(OsRng)?, RESEED_THRESHOLD, OsRng);

    Ok(crate::composite_rng!(rdrand, chacha, xchacha, hc))
}
