use anyhow::Result;
use flume::{Receiver, Sender};
use std::marker::PhantomData;
use std::thread::{self, JoinHandle};

use crate::cancellation_token::CancellationToken;

// We use this trait instead of returning ProductImpl directly so that we can hide the `F` generic parameter
pub trait Producer<T> {
    /// Create a new consumer receiving products from this producer.
    /// There can be multiple consumers and the products will be split
    /// among them, i.e. different consumers get different products.
    fn make_consumer(&self) -> Consumer<T>;
}

pub fn new_producer<T, F>(
    num_workers: usize,
    product_buffer_size: usize,
    make_produce_fn: impl Fn() -> Result<F>,
) -> Result<impl Producer<T>>
where
    T: 'static + Send,
    F: 'static + Send + FnMut() -> Result<T>,
{
    ProducerImpl::new(num_workers, product_buffer_size, make_produce_fn)
}

struct ProducerImpl<T, F>
where
    T: 'static + Send,
    F: 'static + Send + FnMut() -> Result<T>,
{
    receiver: Receiver<T>,
    workers: Vec<Worker>,
    cancellation_token: CancellationToken,
    _f: PhantomData<F>,
}

impl<T, F> ProducerImpl<T, F>
where
    T: 'static + Send,
    F: 'static + Send + FnMut() -> Result<T>,
{
    pub fn new(
        num_workers: usize,
        product_buffer_size: usize,
        make_produce_fn: impl Fn() -> Result<F>,
    ) -> Result<Self> {
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
            _f: PhantomData,
        })
    }
}

impl<T, F> Producer<T> for ProducerImpl<T, F>
where
    T: 'static + Send,
    F: 'static + Send + FnMut() -> Result<T>,
{
    fn make_consumer(&self) -> Consumer<T> {
        Consumer {
            receiver: self.receiver.clone(),
        }
    }
}

impl<T, F> Drop for ProducerImpl<T, F>
where
    T: 'static + Send,
    F: Send + FnMut() -> Result<T>,
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
            self.workers.retain(|join_handle| {
                if join_handle.is_finished() {
                    // This thread is finished, remove it from the list
                    true
                } else {
                    // Keep it in the list
                    false
                }
            });
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

pub struct Consumer<T> {
    receiver: Receiver<T>,
}

impl<T> Consumer<T> {
    pub async fn async_get_product(&self) -> Result<T> {
        let product = self.receiver.recv_async().await;
        Ok(product?)
    }

    pub fn blocking_get_product(&self) -> Result<T> {
        let product = self.receiver.recv();
        Ok(product?)
    }
}
