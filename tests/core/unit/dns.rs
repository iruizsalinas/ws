use super::*;
use crate::net::runtime;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn cancelled_dns_callers_keep_capacity_until_os_work_finishes() {
    runtime::shared().unwrap().block_on(async {
        let (started, started_rx) = mpsc::channel();
        let mut releases = Vec::new();
        let mut callers = Vec::new();
        for _ in 0..LIMIT {
            let (release, wait) = mpsc::channel();
            releases.push(release);
            let started = started.clone();
            callers.push(tokio::spawn(blocking(move || {
                started.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            })));
        }
        for _ in 0..LIMIT {
            started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        }
        for caller in callers {
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
        }
        assert_eq!(slots().available_permits(), 0, "cancellation released a running OS lookup's permit");
        let local = resolve("LOCALHOST.", 12345).await.unwrap();
        assert_eq!(local.len(), 2);
        assert!(local.iter().all(|addr| addr.ip().is_loopback() && addr.port() == 12345));
        assert_eq!(resolve("127.0.0.1", 12345).await.unwrap().len(), 1);
        let (extra, extra_rx) = mpsc::channel();
        let next = tokio::spawn(blocking(move || {
            extra.send(()).unwrap();
            Ok(())
        }));
        assert!(extra_rx.recv_timeout(Duration::from_millis(20)).is_err(), "resolver work escaped the limit");
        for release in releases {
            release.send(()).unwrap();
        }
        tokio::time::timeout(Duration::from_secs(1), next).await.unwrap().unwrap().unwrap();
    });
}
