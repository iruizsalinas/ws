use super::*;

#[test]
fn returned_strings_survive_subsequent_native_calls() {
    let first = out(b"first".to_vec());
    let second = out(b"second".to_vec());
    fail(FATAL, "original error");
    let error = ws_error();
    fail(FATAL, "replacement error");
    let replacement = ws_error();
    assert_eq!(first.bytes(), Some(b"first".as_slice()));
    assert_eq!(second.bytes(), Some(b"second".as_slice()));
    assert_eq!(error.bytes(), Some(b"original error".as_slice()));
    assert_eq!(replacement.bytes(), Some(b"replacement error".as_slice()));
    for s in [first, second, error, replacement, out(vec![]), WsStr::NONE] {
        ws_free(s);
    }
}

#[test]
fn conversion_contexts_are_isolated_and_restore_their_parent() {
    assert!(ws_copy_context().is_null());
    let (mut parent, mut child) = (0u8, 0u8);
    let parent = (&raw mut parent).cast::<c_void>();
    let child = (&raw mut child).cast::<c_void>();
    assert!(ws_set_copy_context(parent).is_null());
    std::thread::spawn(|| {
        assert!(ws_copy_context().is_null());
        let mut local = 0u8;
        let local = (&raw mut local).cast::<c_void>();
        assert!(ws_set_copy_context(local).is_null());
        assert_eq!(ws_copy_context(), local);
        assert_eq!(ws_set_copy_context(std::ptr::null_mut()), local);
    })
    .join()
    .unwrap();
    assert_eq!(ws_set_copy_context(child), parent);
    assert_eq!(ws_set_copy_context(parent), child);
    assert_eq!(ws_copy_context(), parent);
    assert_eq!(ws_set_copy_context(std::ptr::null_mut()), parent);
}
