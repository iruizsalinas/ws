//! One process-wide I/O runtime. No connection creates an operating-system thread.

use std::future::Future;
use std::io;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::runtime::{Builder, Runtime};

pub(crate) fn shared() -> io::Result<&'static Runtime> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let workers = std::thread::available_parallelism().map_or(2, usize::from).clamp(2, 4);
            Builder::new_multi_thread()
                .worker_threads(workers)
                .max_blocking_threads(super::dns::LIMIT)
                .thread_name("ws-io")
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| io::Error::other(e.clone()))
}

pub(super) async fn until<T>(deadline: Instant, task: impl Future<Output = io::Result<T>>) -> io::Result<T> {
    tokio::time::timeout_at(deadline.into(), task)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "handshake timed out"))?
}
