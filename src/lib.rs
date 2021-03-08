//! Rust type for a RAII Counter (counts number of held instances,
//! decrements count on `Drop`), implemented with `Arc<AtomicUsize>`.
//!
//! Useful for tracking the number of holders exist for a handle,
//! tracking the number of transactions that are in-flight, etc.
//!
//! # Additional Features
//! * Counters can have a size, eg. a Counter with `size` 4 adds 4
//! to the count, and removes 4 when dropped.
//!
//! # Demo
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
/// on the number of copies (and their size). The count is automatically updated on Drop.
#[derive(Debug)]
pub struct Counter {
    counter: Arc<AtomicUsize>,
    size: usize,
}

/// A 'weak' Counter that does not affect the count.
#[derive(Clone, Debug)]
pub struct WeakCounter(Arc<AtomicUsize>);

impl Counter {
    /// Create a new Counter with the default size of 1.
    pub fn new() -> Counter {
        Counter::new_with_size(1)
    }

    /// Create a new Counter with specified size. This counter adds
    /// `size` to the count, and will remove `size` from the count
    /// when dropped.
    pub fn new_with_size(size: usize) -> Counter {
        Counter {
            counter: Arc::new(AtomicUsize::new(size)),
            size,
        }
    }

    /// Consume self (causing the count to decrease by `size`)
    /// and return a weak reference to the count through a WeakCounter
    pub fn downgrade(self) -> WeakCounter {
        self.spawn_downgrade()
    }

    /// Create a new WeakCounter without consuming self.
    pub fn spawn_downgrade(&self) -> WeakCounter {
        WeakCounter(Arc::clone(&self.counter))
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
    /// Create a new WeakCounter. This WeakCounter creates a new count
    /// with value: 0 since the WeakCounter has no effect on the count.
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

    /// Create a new Counter with `size` 1 without consuming the
    /// current WeakCounter.
    pub fn spawn_upgrade(&self) -> Counter {
        self.spawn_upgrade_with_size(1)
    }

    /// Creates a new Counter with specified size without consuming the
    /// current WeakCounter.
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

        let counter5 = weak.spawn_upgrade_with_size(5);
        assert_eq!(weak.count(), 5);

        {
            let _moved_counter5 = counter5;
            assert_eq!(weak.count(), 5);
            let _counter1 = weak.spawn_upgrade();
            assert_eq!(weak.count(), 6);
        }

        assert_eq!(weak.count(), 0);
    }

    #[test]
    fn counter_with_size_works() {
        let counter = Counter::new_with_size(4);
        assert_eq!(counter.count(), 4);

        let weak = counter.spawn_downgrade();
        assert_eq!(weak.count(), 4);
        drop(counter);
        assert_eq!(weak.count(), 0);
    }
}
