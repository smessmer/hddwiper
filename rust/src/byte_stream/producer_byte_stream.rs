use anyhow::Result;
use async_trait::async_trait;

use super::block_source::{AsyncBlockSource, SyncBlockSource};
use super::{AsyncByteStream, SyncByteStream};
use crate::producer::ProductReceiver;

/// Creates a [ByteStream] from a [ProductReceiver]. Multiple [ProductReceiver]s can be created from the same [Producer],
/// in which case each [ProductReceiver] will gets different blocks
pub fn byte_stream_from_producer(
    producer: ProductReceiver<Vec<u8>>,
) -> impl SyncByteStream + AsyncByteStream {
    super::block_source_byte_stream::BlockSourceByteStream::new(ProductBlockSource::new(producer))
}

/// Wraps a ProductReceiver<Vec<u8>> and produces an implementation of [BlockSource].
pub struct ProductBlockSource {
    consumer: ProductReceiver<Vec<u8>>,
}

impl ProductBlockSource {
    pub fn new(consumer: ProductReceiver<Vec<u8>>) -> Self {
        Self { consumer }
    }
}

impl SyncBlockSource for ProductBlockSource {
    fn blocking_read(&mut self) -> Result<Vec<u8>> {
        self.consumer.blocking_get_product()
    }
}

#[async_trait]
impl AsyncBlockSource for ProductBlockSource {
    async fn async_read(&mut self) -> Result<Vec<u8>> {
        let product = self.consumer.async_get_product();
        product.await
    }
}
