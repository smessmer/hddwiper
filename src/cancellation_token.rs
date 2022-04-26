use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone)]
pub struct CancellationToken {
    should_cancel: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            should_cancel: Arc::new(false.into()),
        }
    }

    pub fn cancel(&mut self) {
        self.should_cancel.store(true, Ordering::Release);
    }

    pub fn cancelled(&self) -> bool {
        self.should_cancel.load(Ordering::Acquire)
    }
}
