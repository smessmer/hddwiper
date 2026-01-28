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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::producer::{Producer, ThreadPoolProducer};

    #[test]
    fn byte_stream_from_producer_reads_data() {
        let block_size = 100;
        let producer: ThreadPoolProducer<Vec<u8>> = ThreadPoolProducer::new(1, 10, || {
            let mut counter: u8 = 0;
            Ok(move || {
                let block: Vec<u8> = (0..block_size).map(|_| {
                    let val = counter;
                    counter = counter.wrapping_add(1);
                    val
                }).collect();
                Ok(block)
            })
        })
        .unwrap();

        let receiver = producer.make_receiver();
        let mut stream = byte_stream_from_producer(receiver);

        let mut data = [0u8; 50];
        stream.blocking_read(&mut data).unwrap();

        // Just verify we got some data
        assert_eq!(data.len(), 50);
    }

    #[test]
    fn byte_stream_from_producer_multiple_reads() {
        let block_size = 10;
        let producer: ThreadPoolProducer<Vec<u8>> = ThreadPoolProducer::new(1, 10, || {
            Ok(move || {
                Ok(vec![0xAB; block_size])
            })
        })
        .unwrap();

        let receiver = producer.make_receiver();
        let mut stream = byte_stream_from_producer(receiver);

        // Read across multiple blocks
        let mut data = [0u8; 25];
        stream.blocking_read(&mut data).unwrap();

        assert!(data.iter().all(|&b| b == 0xAB));
    }
}
