use anyhow::Result;
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

pub trait ProductReceiver<T> {
    fn blocking_get_product(&self) -> Result<T>;
    fn get_all_available_products(&self) -> Vec<T>;
    fn num_products_in_buffer(&self) -> usize;
}

pub struct ThreadPoolProductReceiver<T: Send> {
    receiver: Receiver<T>,
}

impl<T: Send> ProductReceiver<T> for ThreadPoolProductReceiver<T> {
    fn blocking_get_product(&self) -> Result<T> {
        let product = self.receiver.recv();
        Ok(product?)
    }

    fn get_all_available_products(&self) -> Vec<T> {
        self.receiver.drain().collect()
    }

    fn num_products_in_buffer(&self) -> usize {
        self.receiver.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn producer_creates_products() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let producer: ThreadPoolProducer<usize> = ThreadPoolProducer::new(1, 10, move || {
            let counter = Arc::clone(&counter_clone);
            Ok(move || Ok(counter.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap();

        let receiver = producer.make_receiver();

        // Get a few products
        let p1 = receiver.blocking_get_product().unwrap();
        let p2 = receiver.blocking_get_product().unwrap();
        let p3 = receiver.blocking_get_product().unwrap();

        // Products should be sequential (though order between workers may vary)
        assert!(p1 < 10);
        assert!(p2 < 10);
        assert!(p3 < 10);
    }

    #[test]
    fn multiple_workers_produce() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let producer: ThreadPoolProducer<usize> = ThreadPoolProducer::new(4, 20, move || {
            let counter = Arc::clone(&counter_clone);
            Ok(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            })
        })
        .unwrap();

        let receiver = producer.make_receiver();

        // Get several products
        for _ in 0..10 {
            receiver.blocking_get_product().unwrap();
        }

        // All workers should have produced
        assert!(counter.load(Ordering::SeqCst) >= 10);
    }

    #[test]
    fn receiver_num_products_in_buffer() {
        let producer: ThreadPoolProducer<usize> =
            ThreadPoolProducer::new(1, 100, || Ok(|| Ok(42))).unwrap();

        let receiver = producer.make_receiver();

        // Wait a bit for buffer to fill
        std::thread::sleep(Duration::from_millis(50));

        // Should have some products buffered
        assert!(receiver.num_products_in_buffer() > 0);
    }

    #[test]
    fn get_all_available_products() {
        let producer: ThreadPoolProducer<usize> =
            ThreadPoolProducer::new(1, 100, || Ok(|| Ok(42))).unwrap();

        let receiver = producer.make_receiver();

        // Wait a bit for buffer to fill
        std::thread::sleep(Duration::from_millis(50));

        let products = receiver.get_all_available_products();
        assert!(!products.is_empty());
        assert!(products.iter().all(|&p| p == 42));
    }

    #[test]
    fn multiple_receivers_get_different_products() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let producer: ThreadPoolProducer<usize> = ThreadPoolProducer::new(2, 20, move || {
            let counter = Arc::clone(&counter_clone);
            Ok(move || Ok(counter.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap();

        let receiver1 = producer.make_receiver();
        let receiver2 = producer.make_receiver();

        let mut products1 = Vec::new();
        let mut products2 = Vec::new();

        for _ in 0..5 {
            products1.push(receiver1.blocking_get_product().unwrap());
            products2.push(receiver2.blocking_get_product().unwrap());
        }

        // Receivers should get different products (no duplicates across receivers)
        for p1 in &products1 {
            assert!(
                !products2.contains(p1),
                "Product {} appeared in both receivers",
                p1
            );
        }
    }

    #[test]
    fn producer_drop_stops_workers() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        {
            let _producer: ThreadPoolProducer<usize> = ThreadPoolProducer::new(2, 5, move || {
                let counter = Arc::clone(&counter_clone);
                Ok(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                })
            })
            .unwrap();

            // Let it run a bit
            std::thread::sleep(Duration::from_millis(20));
        }

        // After drop, counter should stop incrementing
        let count_after_drop = counter.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        let count_later = counter.load(Ordering::SeqCst);

        // Counter shouldn't have changed much (workers should have stopped)
        assert!(
            count_later <= count_after_drop + 2,
            "Workers didn't stop after drop"
        );
    }
}
