use anyhow::Result;
use async_trait::async_trait;
use futures::try_join;

use crate::producer::{Producer, ProductReceiver};

/// Generates a composite producer made out of two producers that produce blocks.
/// The producer itself then generates the XOR of the two producers.
/// This producer throws an error if the two producers don't produce blocks of the same size.
pub struct CompositeXorProducer<P1: Producer<Vec<u8>>, P2: Producer<Vec<u8>>> {
    producer1: P1,
    producer2: P2,
}

impl<P1: Producer<Vec<u8>>, P2: Producer<Vec<u8>>> CompositeXorProducer<P1, P2> {
    pub fn new(producer1: P1, producer2: P2) -> Self {
        Self {
            producer1,
            producer2,
        }
    }
}

impl<P1: Producer<Vec<u8>>, P2: Producer<Vec<u8>>> Producer<Vec<u8>>
    for CompositeXorProducer<P1, P2>
{
    type Receiver = CompositeXorProductReceiver<P1::Receiver, P2::Receiver>;

    /// Create a new consumer receiving products from this producer.
    /// There can be multiple consumers and the products will be split
    /// among them, i.e. different consumers get different products.
    fn make_receiver(&self) -> Self::Receiver {
        Self::Receiver {
            receiver1: self.producer1.make_receiver(),
            receiver2: self.producer2.make_receiver(),
        }
    }
}

pub struct CompositeXorProductReceiver<
    R1: ProductReceiver<Vec<u8>> + Send + Sync,
    R2: ProductReceiver<Vec<u8>> + Send + Sync,
> {
    receiver1: R1,
    receiver2: R2,
}

#[async_trait]
impl<R1: ProductReceiver<Vec<u8>> + Send + Sync, R2: ProductReceiver<Vec<u8>> + Send + Sync>
    ProductReceiver<Vec<u8>> for CompositeXorProductReceiver<R1, R2>
{
    async fn async_get_product(&self) -> Result<Vec<u8>> {
        let (mut product1, product2) = try_join!(
            self.receiver1.async_get_product(),
            self.receiver2.async_get_product()
        )?;
        _apply_xor(&mut product1, &product2);

        Ok(product1)
    }

    fn blocking_get_product(&self) -> Result<Vec<u8>> {
        let mut product1 = self.receiver1.blocking_get_product()?;
        let product2 = self.receiver2.blocking_get_product()?;
        _apply_xor(&mut product1, &product2);
        Ok(product1)
    }

    fn try_get_product(&self) -> Result<Vec<u8>> {
        // TODO This isn't great since it can drop product1 if getting product2 fails. Can we somehow push it back into the receiver?
        let mut product1 = self.receiver1.try_get_product()?;
        let product2 = self.receiver2.try_get_product()?;
        _apply_xor(&mut product1, &product2);
        Ok(product1)
    }

    fn num_products_in_buffer(&self) -> usize {
        self.receiver1
            .num_products_in_buffer()
            .min(self.receiver2.num_products_in_buffer())
    }

    fn get_all_available_products(&self) -> Vec<Vec<u8>> {
        let mut result = Vec::with_capacity(self.num_products_in_buffer());
        while let Ok(product) = self.try_get_product() {
            result.push(product);
        }
        result
    }
}

fn _apply_xor(dest: &mut [u8], source: &[u8]) {
    assert_eq!(dest.len(), source.len());
    for i in 0..dest.len() {
        dest[i] ^= source[i];
    }
}
