//! A permanent cancellation signal with race-free asynchronous waiters.

use std::future::{Future, poll_fn};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::task::{Poll, Waker};
use tokio::sync::Notify;

#[derive(Default)]
pub(crate) struct Signal {
    set: AtomicBool,
    notify: Notify,
}

#[cfg(test)]
#[path = "../../tests/core/unit/signal.rs"]
mod tests;

impl Signal {
    /// Sets once and returns whether it was already set.
    pub fn set(&self) -> bool {
        let previous = self.set.swap(true, SeqCst);
        if !previous {
            self.notify.notify_waiters();
        }
        previous
    }

    pub fn is_set(&self) -> bool {
        self.set.load(SeqCst)
    }

    pub async fn wait(&self) {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        // Register before checking the flag so notify_waiters cannot be lost.
        notified.as_mut().enable();
        let mut registered: Option<Waker> = None;
        poll_fn(|cx| {
            if self.is_set() {
                return Poll::Ready(());
            }
            // Notify polls its shared waiter lock even with the same waker.
            // This signal only fires once: an unchanged registration is enough
            // until set() publishes the flag and wakes that registered task.
            if registered.as_ref().is_some_and(|waker| waker.will_wake(cx.waker())) {
                return Poll::Pending;
            }
            let result = notified.as_mut().poll(cx);
            if result.is_pending() {
                registered = Some(cx.waker().clone());
            }
            if self.is_set() { Poll::Ready(()) } else { result }
        })
        .await;
    }
}
