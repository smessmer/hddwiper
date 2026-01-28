use anyhow::Result;
use async_trait::async_trait;

pub trait SyncBlockSource {
    fn blocking_read(&mut self) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait AsyncBlockSource {
    #[allow(dead_code)]
    async fn async_read(&mut self) -> Result<Vec<u8>>;
}

#[cfg(test)]
pub mod testutils {
    use super::*;
    use crate::byte_stream::{testutils::FakeByteStream, AsyncByteStream, SyncByteStream};

    /// A block source that just outputs weak PRNG data
    pub struct FakeBlockSource {
        block_size: usize,
        // Using ChaCha20 because rand::rngs::StdRng says it's not reproducible
        source: FakeByteStream,
    }

    impl FakeBlockSource {
        pub fn new(block_size: usize, seed: u64) -> Self {
            Self {
                block_size,
                source: FakeByteStream::new(seed),
            }
        }
    }

    impl SyncBlockSource for FakeBlockSource {
        fn blocking_read(&mut self) -> Result<Vec<u8>> {
            let mut res = vec![0; self.block_size];
            self.source.blocking_read(&mut res).unwrap();
            Ok(res)
        }
    }
    #[async_trait]
    impl AsyncBlockSource for FakeBlockSource {
        async fn async_read(&mut self) -> Result<Vec<u8>> {
            let mut res = vec![0; self.block_size];
            self.source.async_read(&mut res).await.unwrap();
            Ok(res)
        }
    }
}
