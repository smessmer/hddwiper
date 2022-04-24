use anyhow::Result;
use rand::{rngs::OsRng, RngCore};

use crate::byte_stream::SyncByteStream;

/// A block source that outputs a random stream from the OS random generator
#[derive(Clone, Copy)]
pub struct OsRandomGenerator;

impl SyncByteStream for OsRandomGenerator {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        OsRng.fill_bytes(dest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_read() {
        // Just making sure it doesn't crash
        let mut data = [0u8; 1024];
        OsRandomGenerator.blocking_read(&mut data).unwrap();
    }
}
