use anyhow::Result;
use async_trait::async_trait;

use super::block_source::{AsyncBlockSource, SyncBlockSource};
use super::{AsyncByteStream, SyncByteStream};
use crate::producer::ProductReceiver;

/// Creates a [ByteStream] from a [ProductReceiver]. Multiple [ProductReceiver]s can be created from the same [Producer],
/// in which case each [ProductReceiver] will gets different blocks
pub fn byte_stream_from_producer(
    producer: impl ProductReceiver<Vec<u8>> + Send,
) -> impl SyncByteStream + AsyncByteStream {
    super::block_source_byte_stream::BlockSourceByteStream::new(ProductBlockSource::new(producer))
}

/// Wraps a ProductReceiver<Vec<u8>> and produces an implementation of [BlockSource].
pub struct ProductBlockSource<R: ProductReceiver<Vec<u8>>> {
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

#[async_trait]
impl<R: ProductReceiver<Vec<u8>> + Send> AsyncBlockSource for ProductBlockSource<R> {
    async fn async_read(&mut self) -> Result<Vec<u8>> {
        let product = self.receiver.async_get_product();
        product.await
    }
}
