use anyhow::Result;

pub trait SyncByteStream {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()>;
}

#[cfg(test)]
pub mod testutils {
    use super::*;
    use crate::random::XSalsa20Rng;

    /// A block source that just outputs PRNG data
    pub struct FakeByteStream {
        // Using stream cipher XSalsa20 because rand::rngs::StdRng says it's not reproducible
        // and even rand_chacha, while reproducible, doesn't return the same result for
        // fill_bytes(5) + fill_bytes(5) than for fill_bytes(10). This is because fill_bytes
        // gets whole words and drops any leftover bytes on each call.
        rng: XSalsa20Rng,
    }

    impl FakeByteStream {
        pub fn new(seed: u64) -> Self {
            Self {
                rng: XSalsa20Rng::from_u64_seed(seed),
            }
        }
    }

    impl SyncByteStream for FakeByteStream {
        fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
            self.rng.blocking_read(dest)
        }
    }
}

mod block_source;
mod block_source_byte_stream;
mod producer_byte_stream;
pub use producer_byte_stream::byte_stream_from_producer;
mod byte_stream_producer;
pub use byte_stream_producer::new_byte_stream_thread_pool_producer;
mod xor_byte_stream;
pub use xor_byte_stream::XorByteStream;
