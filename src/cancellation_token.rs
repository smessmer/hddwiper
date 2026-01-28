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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.cancelled());
    }

    #[test]
    fn cancelled_after_cancel() {
        let mut token = CancellationToken::new();
        token.cancel();
        assert!(token.cancelled());
    }

    #[test]
    fn clones_share_cancellation_state() {
        let mut token1 = CancellationToken::new();
        let token2 = token1.clone();

        assert!(!token1.cancelled());
        assert!(!token2.cancelled());

        token1.cancel();

        assert!(token1.cancelled());
        assert!(token2.cancelled());
    }

    #[test]
    fn multiple_cancel_calls_are_idempotent() {
        let mut token = CancellationToken::new();
        token.cancel();
        token.cancel();
        token.cancel();
        assert!(token.cancelled());
    }
}
