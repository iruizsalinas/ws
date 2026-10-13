use super::*;
use std::sync::mpsc::{channel, sync_channel};

#[test]
fn expired_requests_keep_capacity_until_their_events_are_drained() {
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = channel();
    let events = Events::from(tx);
    let permit = Permit::acquire(&count, 1).unwrap();
    let worker = events.admitted(permit.clone());
    let (reply, decision) = sync_channel(1);
    worker
        .send(Event::Request {
            server: 1,
            req: crate::Request {
                path: "/".into(),
                headers: vec![],
                protocols: vec![],
                addr: "127.0.0.1:1".parse().unwrap(),
            },
            reply,
        })
        .unwrap();
    // The handshake timed out, but its request still occupies the hub queue.
    drop(decision);
    drop(worker);
    drop(permit);
    assert!(Permit::acquire(&count, 1).is_none());
    drop(rx.recv().unwrap());
    assert!(Permit::acquire(&count, 1).is_some());
}

#[test]
fn close_releases_capacity_after_worker_and_queued_events_finish() {
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = channel();
    let permit = Permit::acquire(&count, 1).unwrap();
    let events = Events::from(tx).admitted(permit.clone());
    events.send(Event::Message { id: 1, data: vec![1], binary: true }).unwrap();
    events.send(Event::Close { id: 1, code: 1006, reason: String::new() }).unwrap();
    drop(rx.recv().unwrap());
    assert!(Permit::acquire(&count, 1).is_none());
    drop(rx.recv().unwrap());
    assert!(Permit::acquire(&count, 1).is_none(), "network worker still active");
    drop(permit);
    assert!(Permit::acquire(&count, 1).is_some(), "retained closed handle must not hold a slot");
    drop(events);
}
