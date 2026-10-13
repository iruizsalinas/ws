// Lifecycle checks that need private Link state.
use super::*;
use std::sync::mpsc::channel;

#[test]
fn late_frames_cannot_reopen_a_terminated_connection() {
    for close in [false, true] {
        let (tx, rx) = channel();
        let link = Link::new(1, true, 1024, tx);
        link.terminate();
        if close {
            link.closing(Some((1000, String::new())));
        } else {
            link.violation(Fail(1002, "late violation"));
        }
        assert_eq!(link.state(), super::super::State::Closed);
        link.end();
        assert!(matches!(rx.try_recv(), Ok(Event::Close { code: 1006, .. })));
        assert!(rx.try_recv().is_err(), "only one final event may be emitted");
    }
}
