use anyhow::Result;

use crate::byte_stream::SyncByteStream;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod implementation {
    use rdrand::{RdRand, RdSeed};

    use super::*;

    #[derive(Clone)]
    pub struct RdRandGenerator {
        rdrand: RdRand,
    }

    impl RdRandGenerator {
        /// This returns a random generator based on RDRAND if that instruction
        /// is available. Otherwise, it just outputs zeroes. This is secure because
        /// we only use it in an xor composite with other random generators.
        pub fn new_if_supported() -> SyncByteStreamOrZeroes<Self> {
            match RdRand::new() {
                Ok(rdrand) => SyncByteStreamOrZeroes::new_stream(Self { rdrand }),
                Err(err) => {
                    log::warn!("Not able to use RDRAND random generator. Generated keys might be less random. Error message: {}", err);
                    SyncByteStreamOrZeroes::new_zeroes()
                }
            }
        }

        pub fn new_zeroes() -> SyncByteStreamOrZeroes<Self> {
            log::warn!("RDRAND generator is disabled. Generaetd keys might be less random.");
            SyncByteStreamOrZeroes::new_zeroes()
        }
    }

    impl SyncByteStream for RdRandGenerator {
        fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
            self.rdrand.fill_bytes(dest);
            Ok(())
        }
    }

    #[derive(Clone)]
    pub struct RdSeedGenerator {
        rdseed: RdSeed,
    }

    impl RdSeedGenerator {
        /// This returns a random generator based on RDSEED if that instruction
        /// is available. Otherwise, it just outputs zeroes. This is secure because
        /// we only use it in an xor composite with other random generators.
        pub fn new_if_supported() -> SyncByteStreamOrZeroes<Self> {
            match RdSeed::new() {
                Ok(rdseed) => SyncByteStreamOrZeroes::new_stream(Self { rdseed }),
                Err(err) => {
                    log::warn!("Not able to use RDSEED random generator. Generated keys might be less random. Error message: {}", err);
                    SyncByteStreamOrZeroes::new_zeroes()
                }
            }
        }
    }

    impl SyncByteStream for RdSeedGenerator {
        fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
            self.rdseed.fill_bytes(dest);
            Ok(())
        }
    }

    /// SyncByteStreamOrZeroes is a random generator that either generates random values
    /// based on the underlying Some(rdrand), or - if the underlying generator
    /// is None, produces a series of zeroes.
    /// This is used so we can disable RDRAND since not all platforms have a fast RDRAND implementation.
    #[derive(Clone)]
    pub struct SyncByteStreamOrZeroes<G: SyncByteStream + Send> {
        stream: Option<G>,
    }

    impl<G: SyncByteStream + Send> SyncByteStreamOrZeroes<G> {
        pub fn new_stream(stream: G) -> Self {
            Self {
                stream: Some(stream),
            }
        }

        /// This returns a random generator that just generates sequences of zeroes
        pub fn new_zeroes() -> Self {
            Self { stream: None }
        }
    }

    impl<G: SyncByteStream + Send> SyncByteStream for SyncByteStreamOrZeroes<G> {
        fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
            if let Some(stream) = &mut self.stream {
                stream.blocking_read(dest)?;
            } else {
                dest.fill(0);
            }
            Ok(())
        }
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
mod implementation {
    // Platform doesn't support RDRAND/RDSEED, let's just compile dummy implementations

    use super::*;

    #[derive(Clone)]
    pub struct RdRandGenerator;

    impl RdRandGenerator {
        pub fn new_if_supported() -> ZeroesGenerator {
            log::warn!("RDRAND random generator unavailable on this platform. Generated keys might be less random.");
            ZeroesGenerator
        }

        pub fn new_zeroes() -> ZeroesGenerator {
            log::warn!("RDRAND generator is disabled. Generaetd keys might be less random.");
            ZeroesGenerator
        }
    }

    #[derive(Clone)]
    pub struct RdSeedGenerator;

    impl RdSeedGenerator {
        pub fn new_if_supported() -> ZeroesGenerator {
            log::warn!("RDSEED random generator unavailable on this platform. Generated keys might be less random.");
            ZeroesGenerator
        }
    }

    /// "Random" generator that just outputs zeroes
    #[derive(Clone)]
    pub struct ZeroesGenerator;

    impl SyncByteStream for ZeroesGenerator {
        fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
            dest.fill(0);
            Ok(())
        }
    }
}

pub use implementation::RdRandGenerator;
pub use implementation::RdSeedGenerator;
