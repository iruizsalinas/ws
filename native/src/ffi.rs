//! Shared ABI plumbing: strings, status codes and the handle registry.
//!
//! Every object crosses the boundary as an integer id, so C never holds a
//! Rust pointer. Returned strings point into a thread-local buffer that
//! stays valid until the next call on the same thread; C copies them at once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_int;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use ws_core::{Conn, Hub, Id, Reply, Server};

pub const OK: c_int = 0;
/// Expected failures, such as sending on a closed socket: `nil, err`.
pub const SOFT: c_int = 1;
/// Invalid arguments: raised as Lua errors.
pub const FATAL: c_int = 2;

/// A borrowed byte string; a null `ptr` means "absent".
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WsStr {
    ptr: *const u8,
    len: usize,
}

impl WsStr {
    pub const NONE: WsStr = WsStr { ptr: std::ptr::null(), len: 0 };

    pub fn bytes(&self) -> Option<&[u8]> {
        if self.ptr.is_null() {
            return None;
        }
        // SAFETY: the C side passes a Lua string, alive for this whole call.
        Some(unsafe { std::slice::from_raw_parts(self.ptr, self.len) })
    }

    /// The string as UTF-8, or a fatal error naming `what`.
    pub fn text(&self, what: &str) -> Result<Option<&str>, c_int> {
        match self.bytes().map(std::str::from_utf8) {
            None => Ok(None),
            Some(Ok(s)) => Ok(Some(s)),
            Some(Err(_)) => Err(fail(FATAL, &format!("{what} must be valid UTF-8"))),
        }
    }
}

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn hold(cell: &'static std::thread::LocalKey<RefCell<Vec<u8>>>, bytes: Vec<u8>) -> WsStr {
    cell.with(|c| {
        *c.borrow_mut() = bytes;
        let b = c.borrow();
        WsStr { ptr: b.as_ptr(), len: b.len() }
    })
}

/// Returns `bytes` to C, valid until the next call on this thread.
pub fn out(bytes: Vec<u8>) -> WsStr {
    hold(&OUT, bytes)
}

/// Records `message` for `ws_error` and returns `status`.
pub fn fail(status: c_int, message: &str) -> c_int {
    hold(&ERROR, message.as_bytes().to_vec());
    status
}

/// The message of the last failed call on this thread.
#[unsafe(no_mangle)]
pub extern "C" fn ws_error() -> WsStr {
    ERROR.with(|c| {
        let b = c.borrow();
        WsStr { ptr: b.as_ptr(), len: b.len() }
    })
}

#[derive(Default)]
pub struct Registry {
    pub hubs: HashMap<u64, Arc<Hub>>,
    pub conns: HashMap<Id, Conn>,
    pub servers: HashMap<Id, Server>,
    /// Pending upgrade decisions, keyed by token.
    pub replies: HashMap<u64, SyncSender<Reply>>,
    pub next: u64,
}

impl Registry {
    pub fn token(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

pub fn registry() -> MutexGuard<'static, Registry> {
    static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` on a live connection, or reports it as closed.
pub fn with_conn(id: Id, f: impl FnOnce(&Conn) -> c_int) -> c_int {
    let conn = registry().conns.get(&id).cloned();
    match conn {
        Some(c) => f(&c),
        None => fail(SOFT, "closed"),
    }
}
