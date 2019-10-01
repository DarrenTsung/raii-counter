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

use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Essentially an AtomicUsize that is clonable and whose count is based
/// on the number of copies. The count is automatically updated on Drop.
#[derive(Debug)]
pub struct Counter {
    counter: Arc<AtomicUsize>,
    size: usize,
}

/// A 'weak' Counter that does not affect the count.
#[derive(Clone, Debug)]
pub struct WeakCounter(Arc<AtomicUsize>);

impl Counter {
    pub fn new() -> Counter {
        Counter::new_with_size(1)
    }

    pub fn new_with_size(size: usize) -> Counter {
        Counter {
            counter: Arc::new(AtomicUsize::new(1)),
            size,
        }
    }

    /// Consume self (causing the count to decrease by 1)
    /// and return a weak reference to the count through a WeakCounter
    pub fn downgrade(self) -> WeakCounter {
        WeakCounter(self.counter.clone())
    }

    /// This method is inherently racey. Assume the count will have changed once
    /// the value is observed.
    #[inline]
    pub fn count(&self) -> usize {
        self.counter.load(Ordering::Acquire)
    }
}

impl Clone for Counter {
    fn clone(&self) -> Self {
        self.counter.fetch_add(self.size, Ordering::AcqRel);
        Counter {
            counter: self.counter.clone(),
            size: self.size,
        }
    }
}

impl Display for Counter {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Counter(count={})", self.count())
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.size, Ordering::AcqRel);
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
    /// Defaults to a Counter of size 1
    pub fn spawn_upgrade(&self) -> Counter {
        self.spawn_upgrade_with_size(1)
    }

    /// Instead of clone + upgrade, this will only clone once
    pub fn spawn_upgrade_with_size(&self, size: usize) -> Counter {
        self.0.fetch_add(size, Ordering::AcqRel);
        Counter {
            counter: self.0.clone(),
            size,
        }
    }
}

impl Display for WeakCounter {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "WeakCounter(count={})", self.count())
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

    #[test]
    fn different_sizes_work() {
        let weak = WeakCounter::new();
        assert_eq!(weak.count(), 0);

        let counter = weak.spawn_upgrade_with_size(5);
        assert_eq!(weak.count(), 5);

        {
            let _counter1 = counter.clone();
            assert_eq!(weak.count(), 10);
            let _counter2 = weak.spawn_upgrade();
            assert_eq!(weak.count(), 11);
        }

        assert_eq!(weak.count(), 5);
    }
}
