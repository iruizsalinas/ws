// Lifecycle checks that need private Link state.
use super::*;
use crate::net::link::Config;
use std::sync::mpsc::channel;

#[test]
fn late_frames_cannot_reopen_a_terminated_connection() {
    for close in [false, true] {
        let (tx, rx) = channel();
        let link = Link::new(1, true, Config::test(1024), tx.into());
        link.terminate();
        if close {
            link.closing(Some((1000, String::new())));
        } else {
            link.violation(Fail(1002, "late violation"));
        }
        assert_eq!(link.state(), super::super::State::Closed);
        link.end();
        assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Close { code: 1006, .. })));
        assert!(rx.try_recv().is_err(), "only one final event may be emitted");
    }
}

#[test]
fn closing_delivery_does_not_grow_a_full_inbox() {
    let (tx, rx) = channel();
    let link = Link::new(1, true, Config::test(1024), tx.into());
    let limit = INBOX_LIMIT;
    link.inbox.store(limit, Relaxed);
    lock(&link.flags).state = super::super::State::Closing;
    for msg in [Msg::Text(vec![1]), Msg::Binary(vec![1]), Msg::Ping(vec![1]), Msg::Pong(vec![1])] {
        link.handle(msg);
    }
    assert_eq!(link.inbox.load(Relaxed), limit);
    assert!(rx.try_recv().is_err());
}

#[test]
fn sends_apply_backpressure_to_a_stalled_writer() {
    let (tx, _rx) = channel();
    let link = Link::new(1, true, Config { buffer: 64, ..Config::test(1024) }, tx.into());
    lock(&link.flags).state = super::super::State::Open;
    // The buffered count includes the batch currently held by the writer.
    link.buffered.store(64, Relaxed);
    assert_eq!(link.send(frame::TEXT, b"more"), Err("send buffer is full"));
    assert_eq!(link.send(frame::PING, b"more"), Err("send buffer is full"));
    assert_eq!(link.state(), super::super::State::Open);
    // Below the configured limit, a frame is accepted even if it crosses it.
    link.buffered.store(63, Relaxed);
    assert_eq!(link.send(frame::BINARY, &[0; 100]), Ok(()));
}

#[test]
fn drain_is_reported_once_a_refused_send_would_fit() {
    let (tx, rx) = channel();
    let link = Link::new(1, true, Config { buffer: 64, ..Config::test(1024) }, tx.into());
    lock(&link.flags).state = super::super::State::Open;
    link.buffered.store(100, Relaxed);
    assert_eq!(link.send(frame::TEXT, b"x"), Err("send buffer is full"));
    link.progress(30);
    assert!(rx.try_recv().is_err(), "70 bytes left is still over the limit");
    link.progress(10);
    assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Drain { id: 1 })));
    link.progress(10);
    assert!(rx.try_recv().is_err(), "one drain per refusal");
    // Writes nobody was refused for never report a drain.
    link.buffered.store(100, Relaxed);
    link.progress(100);
    assert!(rx.try_recv().is_err());
}
