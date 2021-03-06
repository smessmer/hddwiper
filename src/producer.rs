use anyhow::Result;
use async_trait::async_trait;
use flume::{Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::cancellation_token::CancellationToken;

/// A [Producer] produces products that can be received via one or multiple [ProductReceiver]s.
pub trait Producer<T> {
    // TODO These type bounds here wouldn't be necessary if we just restrict all call sites to them
    type Receiver: ProductReceiver<T> + 'static + Send + Sync;

    /// Create a new consumer receiving products from this producer.
    /// There can be multiple consumers and the products will be split
    /// among them, i.e. different consumers get different products.
    fn make_receiver(&self) -> Self::Receiver;
}

/// A [Producer] for products of type `T` that are produced on a thread pool
pub struct ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    receiver: Receiver<T>,
    workers: Vec<Worker>,
    cancellation_token: CancellationToken,
}

impl<T> ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    pub fn new<F>(
        num_workers: usize,
        product_buffer_size: usize,
        make_produce_fn: impl Fn() -> Result<F>,
    ) -> Result<Self>
    where
        F: 'static + Send + FnMut() -> Result<T>,
    {
        let (sender, receiver) = flume::bounded(product_buffer_size);
        let cancellation_token = CancellationToken::new();
        let workers: Vec<Worker> = (0..num_workers)
            .map(|_| {
                Ok(Worker::new(
                    sender.clone(),
                    make_produce_fn()?,
                    cancellation_token.clone(),
                ))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            receiver,
            workers,
            cancellation_token,
        })
    }
}

impl<T> Producer<T> for ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    type Receiver = ThreadPoolProductReceiver<T>;

    fn make_receiver(&self) -> ThreadPoolProductReceiver<T> {
        ThreadPoolProductReceiver {
            receiver: self.receiver.clone(),
        }
    }
}

impl<T> Drop for ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    fn drop(&mut self) {
        // First, set threads to cancel after they deliver their next product
        self.cancellation_token.cancel();

        // Now, join worker threads
        while !self.workers.is_empty() {
            // Remove all products from the channel so that worker threads aren't blocked on sending products
            while let Ok(_product) = self.receiver.try_recv() {
                // do nothing
            }

            // Remove all workers that already terminated
            self.workers
                .retain(|join_handle| !join_handle.is_finished());
        }
    }
}

struct Worker {
    join_handle: JoinHandle<()>,
}

impl Worker {
    pub fn new<T, F>(
        sender: Sender<T>,
        mut produce_fn: F,
        cancellation_token: CancellationToken,
    ) -> Self
    where
        T: 'static + Send,
        F: 'static + Send + FnMut() -> Result<T>,
    {
        let join_handle = thread::spawn(move || {
            while !cancellation_token.cancelled() {
                // TODO Restart worker on error instead of crashing
                let product = (produce_fn)().unwrap();
                sender.send(product).unwrap();
            }
        });
        Self { join_handle }
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
}

#[async_trait]
pub trait ProductReceiver<T> {
    async fn async_get_product(&self) -> Result<T>;
    fn blocking_get_product(&self) -> Result<T>;
    fn get_all_available_products(&self) -> Vec<T>;
    fn try_get_product(&self) -> Result<T>;
    fn num_products_in_buffer(&self) -> usize;
}

pub struct ThreadPoolProductReceiver<T: Send> {
    receiver: Receiver<T>,
}

#[async_trait]
impl<T: Send> ProductReceiver<T> for ThreadPoolProductReceiver<T> {
    async fn async_get_product(&self) -> Result<T> {
        let product = self.receiver.recv_async().await;
        Ok(product?)
    }

    fn blocking_get_product(&self) -> Result<T> {
        let product = self.receiver.recv();
        Ok(product?)
    }

    fn try_get_product(&self) -> Result<T> {
        let product = self.receiver.try_recv()?;
        Ok(product)
    }

    fn get_all_available_products(&self) -> Vec<T> {
        self.receiver.drain().collect()
    }

    fn num_products_in_buffer(&self) -> usize {
        self.receiver.len()
    }
}
