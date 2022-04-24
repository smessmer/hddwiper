use anyhow::Result;
use async_trait::async_trait;
use rand::RngCore;
use rdrand::{RdRand, RdSeed};

use crate::byte_stream::SyncByteStream;

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
                println!("Warning: Not able to use RDRAND random generator. Generated keys might be less random. Error message: {}", err);
                SyncByteStreamOrZeroes::new_zeroes()
            }
        }
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
    /// This returns a random generator based on RDRAND if that instruction
    /// is available. Otherwise, it just outputs zeroes. This is secure because
    /// we only use it in an xor composite with other random generators.
    pub fn new_if_supported() -> SyncByteStreamOrZeroes<Self> {
        match RdSeed::new() {
            Ok(rdseed) => SyncByteStreamOrZeroes::new_stream(Self { rdseed }),
            Err(err) => {
                println!("Warning: Not able to use RDRAND random generator. Generated keys might be less random. Error message: {}", err);
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

#[async_trait]
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
