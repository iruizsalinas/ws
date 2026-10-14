use super::*;

const IV: Duration = Duration::from_millis(100);

fn at(base: Instant, ms: u64) -> Instant {
    base + Duration::from_millis(ms)
}

#[test]
fn quiet_peer_is_pinged_only_after_a_full_interval() {
    let t = Instant::now();
    assert_eq!(decide(IV, at(t, 99), t, None), Action::Wait);
    assert_eq!(decide(IV, at(t, 100), t, None), Action::Ping);
}

#[test]
fn unanswered_ping_is_dead_only_after_a_full_interval() {
    let t = Instant::now();
    let sent = at(t, 100);
    assert_eq!(decide(IV, at(t, 199), t, Some(sent)), Action::Wait, "no early drop");
    assert_eq!(decide(IV, at(t, 200), t, Some(sent)), Action::Dead);
    // Activity at the very moment of the ping does not answer it.
    assert_eq!(decide(IV, at(t, 200), sent, Some(sent)), Action::Dead);
}

#[test]
fn any_sign_of_life_after_a_ping_answers_it() {
    let t = Instant::now();
    let sent = at(t, 100);
    let reply = at(t, 150);
    // Long past the deadline for the ping, but the peer spoke after it.
    assert_eq!(decide(IV, at(t, 240), reply, Some(sent)), Action::Wait);
    // A new quiet interval after that reply earns a new ping, not a drop.
    assert_eq!(decide(IV, at(t, 250), reply, Some(sent)), Action::Ping);
}

#[test]
fn clock_skew_never_panics_or_drops() {
    let t = Instant::now();
    // `last` slightly ahead of `now` (writer and reader race).
    assert_eq!(decide(IV, t, at(t, 5), None), Action::Wait);
}

#[test]
fn write_progress_does_not_answer_an_outstanding_ping() {
    use crate::net::link::Config;
    use crate::{Event, lock};
    use std::sync::mpsc::channel;

    let (tx, rx) = channel();
    let link = Link::new(1, true, Config { heartbeat: Some(IV), ..Config::test(1024) }, tx.into());
    lock(&link.flags).state = State::Open;
    let now = Instant::now();
    let mut alive = Alive {
        seen: now - IV * 3,
        ping: Some(Probe { queued: now - IV * 2, sent: Arc::new(Mutex::new(Some(now - IV * 2))) }),
    };
    // Bytes accepted by the local socket while a backlog remains aren't a pong.
    link.buffered.store(2, Relaxed);
    link.written.store(u64::try_from(link.born.elapsed().as_nanos()).unwrap(), Relaxed);
    link.progress(1);
    assert!(!link.heartbeat(&mut alive), "local writes must not answer a transmitted ping");
    assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Error { .. })));
    assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Close { .. })));
}

#[test]
fn queued_ping_waits_for_write_progress_but_a_stalled_writer_times_out() {
    use crate::net::link::Config;
    use std::sync::mpsc::channel;

    for progressing in [false, true] {
        let (tx, _rx) = channel();
        let mut link = Link::new(1, true, Config { heartbeat: Some(IV), ..Config::test(1024) }, tx.into());
        let now = Instant::now();
        Arc::get_mut(&mut link).unwrap().born = now - IV * 10;
        lock(&link.flags).state = State::Open;
        let sent = Arc::new(Mutex::new(None));
        let mut alive = Alive { seen: now - IV * 3, ping: Some(Probe { queued: now - IV * 2, sent: sent.clone() }) };
        if progressing {
            link.buffered.store(2, Relaxed);
            link.written.store(u64::try_from(link.born.elapsed().as_nanos()).unwrap(), Relaxed);
            link.progress(1);
        }
        assert_eq!(link.heartbeat(&mut alive), progressing, "queued ping, writer progressing: {progressing}");
        if progressing {
            assert_eq!(link.state(), State::Open, "don't drop a healthy transfer before the ping is written");
            // Once actually written, subsequent local writes cannot extend its response deadline.
            *lock(&sent) = Some(now - IV * 2);
            assert!(!link.heartbeat(&mut alive));
        }
        assert_eq!(link.state(), State::Closed);
    }
}

#[test]
fn a_newly_queued_ping_gets_time_to_leave_a_previously_idle_socket() {
    use crate::net::link::Config;
    use std::sync::mpsc::channel;

    let (tx, _rx) = channel();
    let mut link = Link::new(1, true, Config { heartbeat: Some(IV), ..Config::test(1024) }, tx.into());
    let now = Instant::now();
    Arc::get_mut(&mut link).unwrap().born = now - IV * 10;
    lock(&link.flags).state = State::Open;
    let mut alive = Alive { seen: now - IV * 3, ping: Some(Probe { queued: now, sent: Arc::new(Mutex::new(None)) }) };
    assert!(link.heartbeat(&mut alive));
    assert_eq!(link.state(), State::Open);
}
