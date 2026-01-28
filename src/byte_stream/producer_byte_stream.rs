use anyhow::Result;

use super::block_source::SyncBlockSource;
use super::SyncByteStream;
use crate::producer::ProductReceiver;

/// Creates a [ByteStream] from a [ProductReceiver]. Multiple [ProductReceiver]s can be created from the same [Producer],
/// in which case each [ProductReceiver] will gets different blocks
pub fn byte_stream_from_producer(producer: impl ProductReceiver<Vec<u8>> + Send) -> impl SyncByteStream {
    super::block_source_byte_stream::BlockSourceByteStream::new(ProductBlockSource::new(producer))
}

/// Wraps a ProductReceiver<Vec<u8>> and produces an implementation of [BlockSource].
struct ProductBlockSource<R: ProductReceiver<Vec<u8>>> {
    receiver: R,
}

impl<R: ProductReceiver<Vec<u8>>> ProductBlockSource<R> {
    pub fn new(receiver: R) -> Self {
        Self { receiver }
    }
}

impl<R: ProductReceiver<Vec<u8>>> SyncBlockSource for ProductBlockSource<R> {
    fn blocking_read(&mut self) -> Result<Vec<u8>> {
        self.receiver.blocking_get_product()
    }
}
