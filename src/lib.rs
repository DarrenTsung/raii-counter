use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// This is essentially an AtomicUsize that is clonable and whose count is based
/// on the number of copies. The count is automaticaly updated on drop.
pub struct Counter(Arc<AtomicUsize>);

/// This is a 'weak' reference to the Counter, so it will not affect the count
#[derive(Clone)]
pub struct WeakCounter(Arc<AtomicUsize>);

impl Counter {
    pub fn new() -> Counter {
        Counter(Arc::new(AtomicUsize::new(1)))
    }

    pub fn downgrade(self) -> WeakCounter {
        WeakCounter(self.0.clone())
    }

    /// This method is inherently racey. Assume the count will have changed once
    /// the value is observed.
    #[inline]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

impl Clone for Counter {
    fn clone(&self) -> Self {
        self.0.fetch_add(1, Ordering::AcqRel);
        Counter(self.0.clone())
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl WeakCounter {
    /// This method is inherently racey. Assume the count will have changed once
    /// the value is observed.
    #[inline]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    /// Consumes self, becomes a Counter
    pub fn upgrade(self) -> Counter {
        self.spawn_upgrade()
    }

    /// Instead of clone + upgrade, this will only clone once
    pub fn spawn_upgrade(&self) -> Counter {
        self.0.fetch_add(1, Ordering::AcqRel);
        Counter(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let counter = Counter::new();
        assert_eq!(counter.count(), 1);

        let weak = counter.downgrade();
        assert_eq!(weak.count(), 0);

        {
            let _counter1 = weak.spawn_upgrade();
            assert_eq!(weak.count(), 1);
            let _counter2 = weak.spawn_upgrade();
            assert_eq!(weak.count(), 2);
        }

        assert_eq!(weak.count(), 0);
    }
}
