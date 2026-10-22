use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// A [Producer] produces products that can be received via one or multiple [ProductReceiver]s.
pub trait Producer<T> {
    // TODO These type bounds here wouldn't be necessary if we just restrict all call sites to them
    type Receiver: ProductReceiver<T> + 'static + Send + Sync;

    /// Create a new consumer receiving products from this producer.
    /// There can be multiple consumers and the products will be split
    /// among them, i.e. different consumers get different products.
    fn make_receiver(&self) -> Self::Receiver;
}

/// A [Producer] for products of type `T` that are produced on a thread pool.
///
/// Products are numbered 0, 1, 2, ... and are handed out to the [ProductReceiver]s
/// in that order, no matter which worker finishes first.
pub struct ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    shared: Arc<Shared<T>>,
    workers: Vec<Worker>,
}

/// What the workers and the receivers of a [ThreadPoolProducer] share
struct Shared<T> {
    state: Mutex<State<T>>,

    /// Notified on each change to `state`
    state_changed: Condvar,

    /// The number of finished products receivers can see and take at once. Finished products
    /// beyond that count as still being held by their worker. This limits how many products a
    /// receiver holds on to while the workers are already refilling the buffer.
    product_buffer_size: usize,

    /// Workers don't start a new product while this many products are either still being
    /// produced or are finished but weren't received yet. Without this, workers could run
    /// arbitrarily far ahead of a slow worker whose product the receivers are waiting for.
    max_pending_products: u64,
}

struct State<T> {
    /// Index of the next product that wasn't assigned to a worker yet
    next_to_assign: u64,

    /// Index of the next product a receiver gets
    next_to_deliver: u64,

    /// Finished products that weren't received yet
    finished: BTreeMap<u64, T>,

    /// Products whose worker died while producing them. They get assigned to another worker.
    abandoned: Vec<u64>,

    num_alive_workers: usize,

    /// Set when the workers are supposed to stop
    cancelled: bool,
}

impl<T> State<T> {
    /// Removes the next product in line if it is finished already
    fn take_next(&mut self) -> Option<T> {
        let product = self.finished.remove(&self.next_to_deliver)?;
        self.next_to_deliver += 1;
        Some(product)
    }

    /// The number of finished products that are next in line, i.e. that don't have to wait
    /// for an earlier product to get finished
    fn num_deliverable(&self) -> usize {
        self.finished
            .keys()
            .zip(self.next_to_deliver..)
            .take_while(|&(&index, expected)| index == expected)
            .count()
    }
}

impl<T> Shared<T> {
    /// Blocks until a worker may start with a product and returns the index of that product.
    /// Returns None if the worker is supposed to stop instead.
    fn assign_product(&self) -> Option<u64> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.cancelled {
                return None;
            }
            if let Some(index) = state.abandoned.pop() {
                return Some(index);
            }
            if state.next_to_assign < state.next_to_deliver + self.max_pending_products {
                let index = state.next_to_assign;
                state.next_to_assign += 1;
                return Some(index);
            }
            state = self.state_changed.wait(state).unwrap();
        }
    }

    fn finish_product(&self, index: u64, product: T) {
        self.state.lock().unwrap().finished.insert(index, product);
        self.state_changed.notify_all();
    }

    fn cancel(&self) {
        self.state.lock().unwrap().cancelled = true;
        self.state_changed.notify_all();
    }
}

impl<T> ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    /// Launches `num_workers` workers and calls `make_produce_fn` once for each of them. A worker
    /// calls the function it got with the index of the product it is supposed to produce.
    ///
    /// Up to `product_buffer_size` finished products are kept in a buffer for the receivers. On top
    /// of that, each worker can hold one product, so there are never more than
    /// `product_buffer_size + num_workers` products that weren't received yet.
    pub fn new<F>(
        num_workers: usize,
        product_buffer_size: usize,
        make_produce_fn: impl Fn() -> Result<F>,
    ) -> Result<Self>
    where
        F: 'static + Send + FnMut(u64) -> Result<T>,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                next_to_assign: 0,
                next_to_deliver: 0,
                finished: BTreeMap::new(),
                abandoned: Vec::new(),
                num_alive_workers: num_workers,
                cancelled: false,
            }),
            state_changed: Condvar::new(),
            product_buffer_size,
            max_pending_products: (product_buffer_size + num_workers) as u64,
        });
        // If we fail halfway through, dropping the producer stops the workers we already launched
        let mut producer = Self {
            shared,
            workers: Vec::with_capacity(num_workers),
        };
        for _ in 0..num_workers {
            let worker = Worker::new(Arc::clone(&producer.shared), make_produce_fn()?);
            producer.workers.push(worker);
        }
        Ok(producer)
    }
}

impl<T> Producer<T> for ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    type Receiver = ThreadPoolProductReceiver<T>;

    fn make_receiver(&self) -> ThreadPoolProductReceiver<T> {
        ThreadPoolProductReceiver {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for ThreadPoolProducer<T>
where
    T: 'static + Send,
{
    fn drop(&mut self) {
        // Workers stop after they finished their current product
        self.shared.cancel();

        for worker in self.workers.drain(..) {
            // If the worker panicked, that was already reported when it happened
            let _ = worker.join_handle.join();
        }
    }
}

struct Worker {
    join_handle: JoinHandle<()>,
}

impl Worker {
    pub fn new<T, F>(shared: Arc<Shared<T>>, mut produce_fn: F) -> Self
    where
        T: 'static + Send,
        F: 'static + Send + FnMut(u64) -> Result<T>,
    {
        let join_handle = thread::spawn(move || {
            let mut exit_guard = WorkerExitGuard {
                shared: &shared,
                assigned_product: None,
            };
            while let Some(index) = shared.assign_product() {
                exit_guard.assigned_product = Some(index);
                // TODO Restart worker on error instead of crashing
                let product = (produce_fn)(index).unwrap();
                shared.finish_product(index, product);
                exit_guard.assigned_product = None;
            }
        });
        Self { join_handle }
    }
}

/// Tells the other threads that a worker is gone, no matter if it was stopped or if it panicked
struct WorkerExitGuard<'a, T> {
    shared: &'a Shared<T>,

    /// The product the worker is producing at the moment
    assigned_product: Option<u64>,
}

impl<T> Drop for WorkerExitGuard<'_, T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(index) = self.assigned_product {
            // The worker died while producing. Somebody else has to produce this product,
            // otherwise the receivers would wait for it forever.
            state.abandoned.push(index);
        }
        state.num_alive_workers -= 1;
        drop(state);
        self.shared.state_changed.notify_all();
    }
}

pub trait ProductReceiver<T> {
    fn blocking_get_product(&self) -> Result<T>;
    fn get_all_available_products(&self) -> Vec<T>;
    fn num_products_in_buffer(&self) -> usize;
}

pub struct ThreadPoolProductReceiver<T: Send> {
    shared: Arc<Shared<T>>,
}

impl<T: Send> ProductReceiver<T> for ThreadPoolProductReceiver<T> {
    fn blocking_get_product(&self) -> Result<T> {
        let mut state = self.shared.state.lock().unwrap();
        loop {
            if let Some(product) = state.take_next() {
                self.shared.state_changed.notify_all();
                return Ok(product);
            }
            if state.num_alive_workers == 0 {
                bail!("There are no workers left that could produce the product");
            }
            state = self.shared.state_changed.wait(state).unwrap();
        }
    }

    fn get_all_available_products(&self) -> Vec<T> {
        let mut state = self.shared.state.lock().unwrap();
        let products: Vec<T> = std::iter::from_fn(|| state.take_next())
            .take(self.shared.product_buffer_size)
            .collect();
        if !products.is_empty() {
            self.shared.state_changed.notify_all();
        }
        products
    }

    fn num_products_in_buffer(&self) -> usize {
        let state = self.shared.state.lock().unwrap();
        state.num_deliverable().min(self.shared.product_buffer_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Waits until `condition` holds and fails the test if that takes too long
    fn wait_until(condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "Timed out waiting for condition");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn producer_creates_products() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let producer: ThreadPoolProducer<usize> = ThreadPoolProducer::new(1, 10, move || {
            let counter = Arc::clone(&counter_clone);
            Ok(move |_| Ok(counter.fetch_add(1, Ordering::SeqCst)))
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
            Ok(move |_| {
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
            ThreadPoolProducer::new(1, 100, || Ok(|_| Ok(42))).unwrap();

        let receiver = producer.make_receiver();

        // Wait a bit for buffer to fill
        std::thread::sleep(Duration::from_millis(50));

        // Should have some products buffered
        assert!(receiver.num_products_in_buffer() > 0);
    }

    #[test]
    fn get_all_available_products() {
        let producer: ThreadPoolProducer<usize> =
            ThreadPoolProducer::new(1, 100, || Ok(|_| Ok(42))).unwrap();

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
            Ok(move |_| Ok(counter.fetch_add(1, Ordering::SeqCst)))
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
                Ok(move |_| {
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

    #[test]
    fn products_are_received_in_index_order() {
        // Products with an even index take longer, so workers finish them out of order
        let producer: ThreadPoolProducer<u64> = ThreadPoolProducer::new(7, 3, || {
            Ok(|index: u64| {
                if index % 2 == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(index)
            })
        })
        .unwrap();
        let receiver = producer.make_receiver();

        let mut products = Vec::new();
        while products.len() < 200 {
            products.push(receiver.blocking_get_product().unwrap());
            products.extend(receiver.get_all_available_products());
        }

        let expected: Vec<u64> = (0..products.len() as u64).collect();
        assert_eq!(expected, products);
    }

    #[test]
    fn products_behind_an_unfinished_product_arent_available() {
        // Product 0 doesn't get finished until we allow it
        let first_product_allowed = Arc::new(AtomicBool::new(false));
        let first_product_allowed_clone = Arc::clone(&first_product_allowed);
        let num_produced = Arc::new(AtomicUsize::new(0));
        let num_produced_clone = Arc::clone(&num_produced);
        let producer: ThreadPoolProducer<u64> = ThreadPoolProducer::new(3, 5, move || {
            let first_product_allowed = Arc::clone(&first_product_allowed_clone);
            let num_produced = Arc::clone(&num_produced_clone);
            Ok(move |index: u64| {
                if index == 0 {
                    wait_until(|| first_product_allowed.load(Ordering::SeqCst));
                }
                num_produced.fetch_add(1, Ordering::SeqCst);
                Ok(index)
            })
        })
        .unwrap();
        let receiver = producer.make_receiver();

        // The other workers produce products 1 to 7, which is as far as they may run ahead
        wait_until(|| num_produced.load(Ordering::SeqCst) == 7);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(7, num_produced.load(Ordering::SeqCst));

        // None of them can be received before product 0
        assert_eq!(0, receiver.num_products_in_buffer());
        assert!(receiver.get_all_available_products().is_empty());

        first_product_allowed.store(true, Ordering::SeqCst);
        for expected in 0..8 {
            assert_eq!(expected, receiver.blocking_get_product().unwrap());
        }
    }

    #[test]
    fn workers_stop_producing_when_buffer_is_full() {
        let num_produced = Arc::new(AtomicUsize::new(0));
        let num_produced_clone = Arc::clone(&num_produced);
        let producer: ThreadPoolProducer<u64> = ThreadPoolProducer::new(4, 6, move || {
            let num_produced = Arc::clone(&num_produced_clone);
            Ok(move |index: u64| {
                num_produced.fetch_add(1, Ordering::SeqCst);
                Ok(index)
            })
        })
        .unwrap();
        let receiver = producer.make_receiver();

        // 6 products fit into the buffer and each of the 4 workers holds one more
        wait_until(|| num_produced.load(Ordering::SeqCst) == 10);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(10, num_produced.load(Ordering::SeqCst));

        // Receivers only see the products in the buffer
        wait_until(|| receiver.num_products_in_buffer() == 6);
        assert_eq!(
            vec![0, 1, 2, 3, 4, 5],
            receiver.get_all_available_products()
        );

        // Taking 6 products made room for 6 new ones
        wait_until(|| num_produced.load(Ordering::SeqCst) == 16);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(16, num_produced.load(Ordering::SeqCst));
    }

    #[test]
    fn product_of_a_failed_worker_is_produced_by_another_worker() {
        let already_failed = Arc::new(AtomicBool::new(false));
        let producer: ThreadPoolProducer<u64> = ThreadPoolProducer::new(3, 5, move || {
            let already_failed = Arc::clone(&already_failed);
            Ok(move |index: u64| {
                if index == 5 && !already_failed.swap(true, Ordering::SeqCst) {
                    bail!("Simulated failure");
                }
                Ok(index)
            })
        })
        .unwrap();
        let receiver = producer.make_receiver();

        for expected in 0..50 {
            assert_eq!(expected, receiver.blocking_get_product().unwrap());
        }
    }

    #[test]
    fn receivers_get_an_error_when_all_workers_failed() {
        let producer: ThreadPoolProducer<u64> = ThreadPoolProducer::new(3, 5, || {
            Ok(|index: u64| {
                if index >= 10 {
                    bail!("Simulated failure");
                }
                Ok(index)
            })
        })
        .unwrap();
        let receiver = producer.make_receiver();

        for expected in 0..10 {
            assert_eq!(expected, receiver.blocking_get_product().unwrap());
        }
        assert!(receiver.blocking_get_product().is_err());
        assert!(receiver.get_all_available_products().is_empty());
    }

    #[test]
    fn receivers_get_an_error_after_producer_was_dropped() {
        let producer: ThreadPoolProducer<usize> =
            ThreadPoolProducer::new(2, 5, || Ok(|_| Ok(42))).unwrap();
        let receiver = producer.make_receiver();
        drop(producer);

        // Products that got finished before the workers stopped can still be received
        let num_leftover = std::iter::from_fn(|| receiver.blocking_get_product().ok()).count();
        assert!(num_leftover <= 7);
        assert!(receiver.blocking_get_product().is_err());
    }

    #[test]
    fn new_fails_if_a_produce_fn_cant_be_created() {
        let num_created = AtomicUsize::new(0);
        let producer = ThreadPoolProducer::<usize>::new(3, 5, || {
            if num_created.fetch_add(1, Ordering::SeqCst) == 2 {
                bail!("Simulated failure");
            }
            Ok(|_| Ok(42))
        });

        assert!(producer.is_err());
    }
}
