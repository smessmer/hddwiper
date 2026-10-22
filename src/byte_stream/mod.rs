use anyhow::Result;

pub trait SyncByteStream {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()>;
}

impl SyncByteStream for Box<dyn SyncByteStream + Send> {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        (**self).blocking_read(dest)
    }
}

/// A [SyncByteStream] that can continue at any position of the stream. That is only possible
/// for streams whose content is fixed up front, e.g. because all of it is derived from a seed.
pub trait SeekableByteStream: SyncByteStream {
    /// Lets the next read start at the byte with index `position`, counted from the start of the stream
    fn seek(&mut self, position: u64) -> Result<()>;
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

    impl SeekableByteStream for FakeByteStream {
        fn seek(&mut self, position: u64) -> Result<()> {
            self.rng.seek(position)
        }
    }
}

mod block_source;
mod block_source_byte_stream;
mod producer_byte_stream;
pub use producer_byte_stream::byte_stream_from_producer;
mod byte_stream_producer;
pub use byte_stream_producer::{
    new_byte_stream_thread_pool_producer, new_seekable_byte_stream_thread_pool_producer,
};
mod prefetched_byte_stream;
mod xor_byte_stream;
pub use xor_byte_stream::XorByteStream;
