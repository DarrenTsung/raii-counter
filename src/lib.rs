//! Rust type for a RAII Counter (counts number of held instances,
//! decrements count on `Drop`), implemented with `Arc<AtomicUsize>`.
//!
//! Useful for tracking the number of holders exist for a handle,
//! tracking the number of transactions that are in-flight, etc.
//!
//! # Additional Features
//! * [`Counter`]s can have a size, eg. a [`Counter`] with `size` 4 adds 4
//! to the count, and removes 4 when dropped.
//! * [`NotifyHandle`]s can be used for efficient conditional checking, eg.
//! if you want to wait until there are no in-flight transactions, see:
//! [`CounterBuilder::create_notify`] / [`WeakCounterBuilder::create_notify`]
//! and [`NotifyHandle::wait_until_condition`].
//!
//! # Demo
//!
//! ```rust
//! extern crate raii_counter;
//! use raii_counter::Counter;
//!
//! let counter = Counter::builder().build();
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

use notify::NotifySender;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod notify;

pub use notify::{NotifyError, NotifyHandle, NotifyTimeoutError};

/// Essentially an AtomicUsize that is clonable and whose count is based
/// on the number of copies (and their size). The count is automatically updated on Drop.
///
/// If you want a weak reference to the counter that doesn't affect the count, see:
/// [`WeakCounter`].
#[derive(Debug)]
pub struct Counter {
    counter: Arc<AtomicUsize>,
    notify: Vec<NotifySender>,
    size: usize,
}

/// A 'weak' [`Counter`] that does not affect the count.
#[derive(Clone, Debug)]
pub struct WeakCounter {
    counter: Arc<AtomicUsize>,
    notify: Vec<NotifySender>,
}

/// A builder for the [`Counter`].
pub struct CounterBuilder {
    counter: Arc<AtomicUsize>,
    size: usize,
    notify: Vec<NotifySender>,
}

impl CounterBuilder {
    /// Change the specified size of the new [`Counter`]. This counter will add
    /// `size` to the count, and will remove `size` from the count
    /// when dropped.
    pub fn size(mut self, v: usize) -> Self {
        self.size = v;
        self
    }

    /// Create a [`NotifyHandle`] with a link to the count of this object. This [`NotifyHandle`] will
    /// be notified when the value of this count changes.
    ///
    /// [`NotifyHandle`]s cannot be associated after creation, since all linked
    /// [`Counter`] / [`WeakCounter`]s cannot be accounted for.
    pub fn create_notify(&mut self) -> NotifyHandle {
        let (handle, sender) = NotifyHandle::new(Arc::clone(&self.counter));
        self.notify.push(sender);
        handle
    }

    /// Create a new [`Counter`].
    pub fn build(self) -> Counter {
        self.counter.fetch_add(self.size, Ordering::SeqCst);
        Counter {
            counter: self.counter,
            notify: self.notify,
            size: self.size,
        }
    }
}

impl Default for CounterBuilder {
    fn default() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            size: 1,
            notify: vec![],
        }
    }
}

impl Counter {
    /// Create a new default [`CounterBuilder`].
    pub fn builder() -> CounterBuilder {
        CounterBuilder::default()
    }

    /// Consume self (causing the count to decrease by `size`)
    /// and return a weak reference to the count through a [`WeakCounter`].
    pub fn downgrade(self) -> WeakCounter {
        self.spawn_downgrade()
    }

    /// Create a new [`WeakCounter`] without consuming self.
    pub fn spawn_downgrade(&self) -> WeakCounter {
        WeakCounter {
            notify: self.notify.clone(),
            counter: Arc::clone(&self.counter),
        }
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
        self.counter.fetch_add(self.size, Ordering::SeqCst);
        for sender in &self.notify {
            sender.notify();
        }
        Counter {
            notify: self.notify.clone(),
            counter: Arc::clone(&self.counter),
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
        self.counter.fetch_sub(self.size, Ordering::SeqCst);
        for sender in &self.notify {
            sender.notify();
        }
    }
}

/// A builder for the [`WeakCounter`].
pub struct WeakCounterBuilder {
    counter: Arc<AtomicUsize>,
    notify: Vec<NotifySender>,
}

impl WeakCounterBuilder {
    /// Create a [`NotifyHandle`] with a link to the count of this object. This [`NotifyHandle`] will
    /// be notified when the value of this count changes.
    ///
    /// [`NotifyHandle`]s cannot be associated after creation, since all linked
    /// [`Counter`] / [`WeakCounter`]s cannot be accounted for.
    pub fn create_notify(&mut self) -> NotifyHandle {
        let (handle, sender) = NotifyHandle::new(Arc::clone(&self.counter));
        self.notify.push(sender);
        handle
    }

    /// Create a new [`WeakCounter`]. This [`WeakCounter`] creates a new count
    /// with value: 0 since the [`WeakCounter`] has no effect on the count.
    pub fn build(self) -> WeakCounter {
        WeakCounter {
            notify: self.notify,
            counter: self.counter,
        }
    }
}

impl Default for WeakCounterBuilder {
    fn default() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            notify: vec![],
        }
    }
}

impl WeakCounter {
    /// Create a new default [`WeakCounterBuilder`].
    pub fn builder() -> WeakCounterBuilder {
        WeakCounterBuilder::default()
    }

    /// This method is inherently racey. Assume the count will have changed once
    /// the value is observed.
    #[inline]
    pub fn count(&self) -> usize {
        self.counter.load(Ordering::Acquire)
    }

    /// Consumes self, becomes a [`Counter`] of `size` 1.
    pub fn upgrade(self) -> Counter {
        self.spawn_upgrade()
    }

    /// Create a new [`Counter`] with `size` 1 without consuming the
    /// current [`WeakCounter`].
    pub fn spawn_upgrade(&self) -> Counter {
        self.spawn_upgrade_with_size(1)
    }

    /// Creates a new [`Counter`] with specified size without consuming the
    /// current [`WeakCounter`].
    pub fn spawn_upgrade_with_size(&self, size: usize) -> Counter {
        self.counter.fetch_add(size, Ordering::SeqCst);
        for sender in &self.notify {
            sender.notify();
        }
        Counter {
            notify: self.notify.clone(),
            counter: Arc::clone(&self.counter),
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
    use std::thread;
    use std::time::Duration;

    #[test]
    fn it_works() {
        let counter = Counter::builder().build();
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
        let weak = WeakCounter::builder().build();
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
        let counter = Counter::builder().size(4).build();
        assert_eq!(counter.count(), 4);

        let weak = counter.spawn_downgrade();
        assert_eq!(weak.count(), 4);
        drop(counter);
        assert_eq!(weak.count(), 0);
    }

    #[test]
    fn wait_until_condition_works() {
        run_wait_until_condition_test(|notify| notify.wait_until_condition(|v| v == 10).unwrap());
    }

    #[test]
    fn wait_until_condition_with_timeout_works() {
        run_wait_until_condition_test(|notify| {
            notify
                .wait_until_condition_timeout(|v| v == 10, Duration::from_secs(2))
                .unwrap()
        });
    }

    fn run_wait_until_condition_test(notify_fn: impl Fn(NotifyHandle)) {
        let (weak, notify) = {
            let mut builder = WeakCounter::builder();
            let notify = builder.create_notify();
            (builder.build(), notify)
        };

        let join_handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let mut counters = vec![];
            for _ in 0..10 {
                counters.push(weak.spawn_upgrade());
            }

            // Return counters from the thread so they
            // never get dropped (at least until the thread
            // gets joined).
            counters
        });

        notify_fn(notify);
        join_handle.join().unwrap();
    }

    /// Run this test to gain more confidence that the notify is not flakey due to
    /// race-conditions.
    ///
    /// ```
    /// cargo test --release -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn test_wait_until_condition_always_occurs() {
        let mut i = 0;
        loop {
            wait_until_condition_works();
            println!("[{}] Completed.", i);
            i += 1;
        }
    }

    #[test]
    fn notify_errors_when_all_references_are_dropped() {
        let (weak, notify) = {
            let mut builder = WeakCounter::builder();
            let notify = builder.create_notify();
            (builder.build(), notify)
        };

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let mut counters = vec![];
            for _ in 0..5 {
                counters.push(weak.spawn_upgrade());
            }
            // All references are dropped here, therefore the condition
            // will never be true.
        });

        assert_eq!(
            notify.wait_until_condition(|v| v == 10),
            Err(NotifyError::Disconnected),
        );
    }

    #[test]
    fn notify_checks_condition_before_erroring() {
        let (weak, notify) = {
            let mut builder = WeakCounter::builder();
            let notify = builder.create_notify();
            (builder.build(), notify)
        };

        // All counter references are dropped.
        drop(weak);

        // Shouldn't error since the condition is true.
        assert!(notify.wait_until_condition(|v| v == 0).is_ok());
    }

    #[test]
    fn notify_with_timeout_can_timeout() {
        let (weak, notify) = {
            let mut builder = WeakCounter::builder();
            let notify = builder.create_notify();
            (builder.build(), notify)
        };

        assert_eq!(
            notify.wait_until_condition_timeout(|v| v == 10, Duration::from_millis(100)),
            Err(NotifyTimeoutError::Timeout)
        );

        // Counters are not dropped until here.
        drop(weak);
    }
}
