// Lifecycle checks that need private Link state.
use super::super::send::Item;
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
    // A frame must fit in the room left: 10 + 52 + 2 header bytes is 64.
    link.buffered.store(10, Relaxed);
    assert_eq!(link.send(frame::BINARY, &[0; 52]), Ok(()));
    assert_eq!(link.send(frame::BINARY, &[0; 53]), Err("send buffer is full"));
    // One larger than the whole buffer goes only when nothing is waiting.
    link.buffered.store(1, Relaxed);
    assert_eq!(link.send(frame::BINARY, &[0; 100]), Err("send buffer is full"));
    link.buffered.store(0, Relaxed);
    assert_eq!(link.send(frame::BINARY, &[0; 100]), Ok(()));
}

#[test]
fn small_writes_share_queue_entries() {
    let (tx, _rx) = channel();
    let link = Link::new(1, true, Config::test(1024), tx.into());
    // Empty pongs, as a peer that pings but never reads would cause.
    for _ in 0..10_000 {
        link.enqueue(Item::Data(frame::encode(frame::PONG, &[], None)));
    }
    let big = vec![0; 64 * 1024];
    link.enqueue(Item::Data(big.clone()));
    link.enqueue(Item::Data(vec![1]));
    link.enqueue(Item::Shutdown);
    link.enqueue(Item::Data(vec![2]));
    let q = lock(&link.queue);
    let sizes: Vec<usize> = q.items.iter().map(|i| if let Item::Data(d) = i { d.len() } else { 0 }).collect();
    // 20,000 bytes: one entry fills up to 16 KiB, the next takes the rest.
    assert_eq!(sizes, [16 * 1024, 20_000 - 16 * 1024, big.len(), 1, 0, 1]);
    assert_eq!(link.buffered.load(Relaxed), 20_000 + big.len() + 2);
    // Order is kept: the bytes are the frames back to back.
    let Item::Data(first) = &q.items[0] else { panic!("expected data") };
    assert!(first.chunks(2).all(|c| c == [0x8A, 0]));
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
    // A frame larger than the buffer fits once nothing is left.
    link.buffered.store(10, Relaxed);
    assert_eq!(link.send(frame::BINARY, &[0; 100]), Err("send buffer is full"));
    link.progress(9);
    assert!(rx.try_recv().is_err(), "1 byte left and 102 more would not fit");
    link.progress(1);
    assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Drain { id: 1 })));
}
