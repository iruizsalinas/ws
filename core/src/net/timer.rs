//! Reuse a timer, registering or resetting it only when a read must wait.
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;
use std::time::Instant;
use tokio::time::Sleep;

#[cfg(test)]
#[path = "../../../tests/core/unit/timer.rs"]
mod tests;

#[derive(Default)]
pub(super) struct Timer {
    target: Option<Instant>,
    armed: Option<(Instant, Pin<Box<Sleep>>)>,
}

impl Timer {
    pub fn set(&mut self, deadline: Option<Instant>) {
        self.target = deadline;
    }

    pub async fn wait(&mut self) {
        poll_fn(|cx| {
            let Some(target) = self.target else { return Poll::Pending };
            let (previous, sleep) =
                self.armed.get_or_insert_with(|| (target, Box::pin(tokio::time::sleep_until(target.into()))));
            if *previous != target {
                sleep.as_mut().reset(target.into());
                *previous = target;
            }
            sleep.as_mut().poll(cx)
        })
        .await
    }
}
