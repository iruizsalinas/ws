use super::*;
use crate::net::runtime;
use std::time::Duration;

#[test]
fn a_cancelled_wait_reuses_its_timer_and_respects_new_deadlines() {
    runtime::shared().unwrap().block_on(async {
        let mut timer = Timer::default();
        timer.set(Some(Instant::now() + Duration::from_secs(60)));
        assert!(tokio::time::timeout(Duration::from_millis(10), timer.wait()).await.is_err());
        timer.set(Some(Instant::now()));
        tokio::time::timeout(Duration::from_secs(1), timer.wait()).await.unwrap();
        timer.set(None);
        assert!(tokio::time::timeout(Duration::from_millis(10), timer.wait()).await.is_err());
        timer.set(Some(Instant::now() + Duration::from_secs(60)));
        assert!(tokio::time::timeout(Duration::from_millis(10), timer.wait()).await.is_err());
        timer.set(Some(Instant::now()));
        tokio::time::timeout(Duration::from_secs(1), timer.wait()).await.unwrap();
    });
}
