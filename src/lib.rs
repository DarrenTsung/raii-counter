//! # raii-counter
//! Rust type for a RAII Counter (counts number of held instances,
//! decrements count on `Drop`), implemented with `Arc<AtomicUsize>`.
//!
//! Useful for tracking the number of holders exist for a handle,
//! tracking the number of transactions that are in-flight, etc.
//!
//! ## Demo
//!
//! ```rust
//! extern crate raii_counter;
//! use raii_counter::Counter;
//!
//! let counter = Counter::new();
//! assert_eq!(counter.count(), 1);
//!
//! let weak = counter.downgrade();
//! assert_eq!(weak.count(), 0);
//!
//! {
//!     let _counter1 = weak.spawn_upgrade();
//!     assert_eq!(weak.count(), 1);
//!     let _counter2 = weak.spawn_upgrade();
//!     assert_eq!(weak.count(), 2);
//! }
//!
//! assert_eq!(weak.count(), 0);
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Essentially an AtomicUsize that is clonable and whose count is based
/// on the number of copies. The count is automatically updated on Drop.
pub struct Counter(Arc<AtomicUsize>);

/// A 'weak' Counter that does not affect the count.
#[derive(Clone)]
pub struct WeakCounter(Arc<AtomicUsize>);

impl Counter {
    pub fn new() -> Counter {
        Counter(Arc::new(AtomicUsize::new(1)))
    }

    /// Consume self (causing the count to decrease by 1)
    /// and return a weak reference to the count through a WeakCounter
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
    pub fn new() -> WeakCounter {
        WeakCounter(Arc::new(AtomicUsize::new(0)))
    }

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
