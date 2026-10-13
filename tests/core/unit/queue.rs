use super::*;
use std::sync::mpsc::{channel, sync_channel};

#[test]
fn closed_hubs_refuse_new_events() {
    let (tx, rx) = channel();
    let events = Events::from(tx);
    let worker = events.clone();
    events.close();
    assert!(worker.send(Event::Close { id: 1, code: 1006, reason: String::new() }).is_err());
    assert!(rx.try_recv().is_err());
}

#[test]
fn a_closed_hub_still_releases_the_connection_slot_on_close() {
    let count = Arc::new(AtomicUsize::new(0));
    let (tx, _rx) = channel();
    let events = Events::from(tx).admitted(Permit::acquire(&count, 1).unwrap());
    events.close();
    assert!(events.send(Event::Close { id: 1, code: 1006, reason: String::new() }).is_err());
    assert!(Permit::acquire(&count, 1).is_some());
}

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
fn expired_requests_retain_pending_slots_even_with_unlimited_connections() {
    let connections = Arc::new(AtomicUsize::new(0));
    let pending = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = channel();
    let events = Events::from(tx);
    let connection = Permit::acquire(&connections, usize::MAX).unwrap();
    let worker = events.admitted(connection.clone());
    let slot = Permit::acquire(&pending, 1).unwrap();
    let (reply, decision) = sync_channel(1);
    worker
        .send_pending(
            Event::Request {
                server: 1,
                req: crate::Request {
                    path: "/".into(),
                    headers: vec![],
                    protocols: vec![],
                    addr: "127.0.0.1:1".parse().unwrap(),
                },
                reply,
            },
            Some(slot.clone()),
        )
        .unwrap();
    // Simulate a timeout without polling the application's event queue.
    drop(decision);
    drop(worker);
    drop(connection);
    drop(slot);
    assert!(Permit::acquire(&connections, usize::MAX).is_some());
    assert!(Permit::acquire(&pending, 1).is_none(), "expired request lost its handshake slot");
    drop(rx.recv().unwrap());
    assert!(Permit::acquire(&pending, 1).is_some(), "draining the request must restore capacity");
    assert_eq!(connections.load(Relaxed), 0);
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
