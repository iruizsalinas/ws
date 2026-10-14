//! An upgrade decision delivered to an async worker without blocking a thread.

use crate::{Reply, lock};
use std::sync::mpsc::SendError;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Clone)]
pub struct ReplySender(Arc<Mutex<Option<oneshot::Sender<Reply>>>>);

impl ReplySender {
    /// Answers once. Late or repeated decisions return their unsent value.
    pub fn send(&self, reply: Reply) -> Result<(), SendError<Reply>> {
        let sender = lock(&self.0).take();
        match sender {
            Some(sender) => sender.send(reply).map_err(SendError),
            None => Err(SendError(reply)),
        }
    }
}

pub(crate) fn channel() -> (ReplySender, oneshot::Receiver<Reply>) {
    let (sender, receiver) = oneshot::channel();
    (ReplySender(Arc::new(Mutex::new(Some(sender)))), receiver)
}
