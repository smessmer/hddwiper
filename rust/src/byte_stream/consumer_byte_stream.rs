use anyhow::Result;
use async_trait::async_trait;

use super::block_source::{SyncBlockSource, AsyncBlockSource};
use super::{SyncByteStream, AsyncByteStream};
use crate::producer::Consumer;

/// Creates a [ByteStream] from a [Consumer]. Multiple [Consumers]s can be created from the same [Producer],
/// in which case each [Consumer] will gets different blocks
pub fn byte_stream_from_consumer(producer: Consumer<Vec<u8>>) -> impl SyncByteStream + AsyncByteStream {
    super::block_source_byte_stream::BlockSourceByteStream::new(ConsumerBlockSource::new(producer))
}

/// Wraps a Consumer<Vec<u8>> and produces an implementation of [BlockSource].
pub struct ConsumerBlockSource {
    consumer: Consumer<Vec<u8>>,
}

impl ConsumerBlockSource {
    pub fn new(consumer: Consumer<Vec<u8>>) -> Self {
        Self { consumer }
    }
}

impl SyncBlockSource for ConsumerBlockSource {
    fn blocking_read(&mut self) -> Result<Vec<u8>> {
        self.consumer.blocking_get_product()
    }
}

#[async_trait]
impl AsyncBlockSource for ConsumerBlockSource {
    async fn async_read(&mut self) -> Result<Vec<u8>> {
        let product = self.consumer.async_get_product();
        product.await
    }
}
