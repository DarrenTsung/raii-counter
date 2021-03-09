use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Struct that enables functionality like waiting to be notified
/// when the count of a [`crate::Counter`] or [`crate::WeakCounter`] changes.
#[derive(Debug)]
pub struct NotifyHandle {
    receiver: mpsc::Receiver<()>,
    should_send: Arc<AtomicBool>,
    counter: Arc<AtomicUsize>,
}

#[derive(Error, Debug, PartialEq, Clone, Copy)]
pub enum NotifyError {
    #[error("All linked senders are disconnected, therefore count will never change!")]
    Disconnected,
}

#[derive(Error, Debug, PartialEq, Clone, Copy)]
pub enum NotifyTimeoutError {
    #[error("All linked senders are disconnected, therefore count will never change!")]
    Disconnected,
    #[error("Timed out before condition was reached!")]
    Timeout,
}

/// Struct that can send signals to the [`NotifyHandle`].
#[derive(Debug, Clone)]
pub(crate) struct NotifySender {
    should_send: Arc<AtomicBool>,
    sender: mpsc::Sender<()>,
}

impl NotifyHandle {
    /// Create a new [`NotifyHandle`] with a link to the associated count.
    pub(crate) fn new(counter: Arc<AtomicUsize>) -> (NotifyHandle, NotifySender) {
        // Create a new "rendezvous channel". Note that we don't
        // buffer any data in the channel, so memory won't grow if
        // no-one is receiving any data.
        let (sender, receiver) = mpsc::channel();
        let should_send = Arc::new(AtomicBool::new(false));
        (
            NotifyHandle {
                receiver,
                should_send: Arc::clone(&should_send),
                counter,
            },
            NotifySender {
                sender,
                should_send,
            },
        )
    }

    /// Block the current thread until the condition is true. This is
    /// different than spin-looping since the current thread will use channels
    /// internally to be notified when the counter changes.
    pub fn wait_until_condition(
        &self,
        condition: impl Fn(usize) -> bool,
    ) -> Result<(), NotifyError> {
        self.wait_until_condition_inner(condition, None)
            .map_err(|e| match e {
                NotifyTimeoutError::Disconnected => NotifyError::Disconnected,
                NotifyTimeoutError::Timeout => {
                    panic!("Timeout error from wait_until_condition without timeout!")
                }
            })
    }

    /// [`NotifyHandle::wait_until_condition`] with a timeout.
    pub fn wait_until_condition_timeout(
        &self,
        condition: impl Fn(usize) -> bool,
        timeout: Duration,
    ) -> Result<(), NotifyTimeoutError> {
        self.wait_until_condition_inner(condition, Some(timeout))
    }

    fn wait_until_condition_inner(
        &self,
        condition: impl Fn(usize) -> bool,
        timeout: Option<Duration>,
    ) -> Result<(), NotifyTimeoutError> {
        let start = Instant::now();

        // Drain all messages in the channel before turning sends on again.
        while let Ok(()) = self.receiver.try_recv() {}
        self.should_send.store(true, Ordering::SeqCst);

        macro_rules! return_if_condition {
            () => {
                if condition(self.counter.load(Ordering::SeqCst)) {
                    self.should_send.store(false, Ordering::SeqCst);
                    return Ok(());
                }
            };
        }

        return_if_condition!();
        loop {
            // Drain all elements from the channel until it's empty. If there were no
            // elements drained, we block on `recv()`.
            let recv_result = {
                let mut received_at_least_once = false;
                loop {
                    match self.receiver.try_recv() {
                        Ok(()) => received_at_least_once = true,
                        Err(mpsc::TryRecvError::Empty) => {
                            if received_at_least_once {
                                break Ok(());
                            }

                            if let Some(timeout) = timeout {
                                let remaining_time = if let Some(remaining_time) =
                                    start.elapsed().checked_sub(timeout)
                                {
                                    remaining_time
                                } else {
                                    break Err(mpsc::RecvTimeoutError::Timeout);
                                };

                                break self.receiver.recv_timeout(remaining_time);
                            } else {
                                break self
                                    .receiver
                                    .recv()
                                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected);
                            }
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            break Err(mpsc::RecvTimeoutError::Disconnected)
                        }
                    }
                }
            };

            // If the receiver thread is disconnected, then the counter
            // will never change again.
            if let Err(err) = recv_result {
                // We should check if the condition is satisfied one last time, then
                // return Disconnected if still unsatisfied, since the condition will
                // never be met.
                return_if_condition!();

                self.should_send.store(false, Ordering::SeqCst);
                return Err(match err {
                    mpsc::RecvTimeoutError::Disconnected => NotifyTimeoutError::Disconnected,
                    mpsc::RecvTimeoutError::Timeout => NotifyTimeoutError::Timeout,
                });
            }

            return_if_condition!();
        }
    }
}

impl NotifySender {
    /// Notify the handle.
    pub(crate) fn notify(&self) {
        if self.should_send.load(Ordering::SeqCst) {
            let _ = self.sender.send(());
        }
    }
}
