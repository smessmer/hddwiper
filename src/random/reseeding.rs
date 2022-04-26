use anyhow::Result;
use generic_array::{ArrayLength, GenericArray};

use crate::byte_stream::SyncByteStream;

pub trait SeedableRandomGenerator: SyncByteStream + Sized {
    type SeedSize: ArrayLength<u8>;

    fn from_seed(seed: GenericArray<u8, Self::SeedSize>) -> Self;
}

pub struct ReseedingRandomGenerator<G: SeedableRandomGenerator + Send, S: SyncByteStream + Send> {
    generator: Option<G>,
    seed_source: S,
    reseed_every_n_bytes: usize,
    bytes_until_reseed: usize,
}

impl<G: SeedableRandomGenerator + Send, S: SyncByteStream + Send> ReseedingRandomGenerator<G, S> {
    pub fn new(reseed_every_n_bytes: usize, seed_source: S) -> Self {
        Self {
            generator: None,
            seed_source,
            reseed_every_n_bytes,
            bytes_until_reseed: 0,
        }
    }

    fn _reseed(&mut self) -> Result<()> {
        log::debug!("Reseeding...");
        let mut new_seed = GenericArray::default();
        self.seed_source.blocking_read(&mut new_seed)?;
        self.generator = Some(G::from_seed(new_seed));
        self.bytes_until_reseed = self.reseed_every_n_bytes;
        log::debug!("Reseeding...finished");
        Ok(())
    }

    fn _read_without_reseed(&mut self, dest: &mut [u8]) -> Result<()> {
        assert!(
            dest.len() <= self.bytes_until_reseed,
            "Not enough bytes left before reseeding necessary"
        );
        self.generator
            .as_mut()
            .expect("Generator not initialized")
            .blocking_read(dest)?;
        self.bytes_until_reseed -= dest.len();
        Ok(())
    }
}

impl<G: SeedableRandomGenerator + Send, S: SyncByteStream + Send + Clone> Clone
    for ReseedingRandomGenerator<G, S>
{
    fn clone(&self) -> Self {
        Self::new(self.reseed_every_n_bytes, self.seed_source.clone())
    }
}

impl<G: SeedableRandomGenerator + Send, S: SyncByteStream + Send> SyncByteStream
    for ReseedingRandomGenerator<G, S>
{
    fn blocking_read(&mut self, mut dest: &mut [u8]) -> Result<()> {
        while dest.len() > self.bytes_until_reseed {
            // Read as much as we can before reseed
            if self.bytes_until_reseed > 0 {
                self._read_without_reseed(&mut dest[..self.bytes_until_reseed])?;

                // Now only try to read into the rest of dest
                dest = &mut dest[self.bytes_until_reseed..];
            }

            self._reseed()?;
            // self._reseed also reset self.bytes_until_reseed, so we can go into the next loop iteration.
        }

        self._read_without_reseed(dest)?;

        Ok(())
    }
}
