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
