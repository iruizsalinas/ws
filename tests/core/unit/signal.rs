use super::*;
use crate::net::runtime;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn cancellation_wakes_registered_and_late_waiters() {
    runtime::shared().unwrap().block_on(async {
        for _ in 0..50 {
            let signal = Arc::new(Signal::default());
            let barrier = Arc::new(tokio::sync::Barrier::new(17));
            let mut tasks = Vec::new();
            for _ in 0..16 {
                let signal = signal.clone();
                let barrier = barrier.clone();
                tasks.push(tokio::spawn(async move {
                    barrier.wait().await;
                    signal.wait().await;
                }));
            }
            barrier.wait().await;
            tokio::task::yield_now().await;
            assert!(!signal.set(), "first setter owns cancellation");
            assert!(signal.set(), "repeated setters must observe the first");
            for task in tasks {
                tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
            }
            tokio::time::timeout(Duration::from_secs(1), signal.wait()).await.unwrap();
        }
    });
}

#[test]
fn a_persistent_wait_registers_a_replacement_task_waker() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::task::{Context, Wake, Waker};
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, SeqCst);
        }
    }
    let first = Arc::new(Count(AtomicUsize::new(0)));
    let second = Arc::new(Count(AtomicUsize::new(0)));
    let first_waker = Waker::from(first.clone());
    let second_waker = Waker::from(second.clone());
    let signal = Signal::default();
    let mut wait = Box::pin(signal.wait());
    for _ in 0..100 {
        assert!(wait.as_mut().poll(&mut Context::from_waker(&first_waker)).is_pending());
    }
    assert!(wait.as_mut().poll(&mut Context::from_waker(&second_waker)).is_pending());
    signal.set();
    assert_eq!(first.0.load(SeqCst), 0, "the replaced task must not be woken");
    assert_eq!(second.0.load(SeqCst), 1, "the current task lost cancellation");
    assert!(wait.as_mut().poll(&mut Context::from_waker(&second_waker)).is_ready());
}
