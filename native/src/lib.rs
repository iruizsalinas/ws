//! The C ABI that `ws.c` exposes to Lua as the `ws.core` module.

mod conn;
mod event;
mod ffi;
mod hub;
mod server;

#[cfg(glue)]
mod glue {
    use std::ffi::{c_int, c_void};

    unsafe extern "C" {
        fn ws_open(lua: *mut c_void) -> c_int;
    }

    /// `require("ws.core")`. Exported from Rust so the symbol survives the
    /// linker's export list; the C glue builds the module table.
    #[unsafe(no_mangle)]
    pub extern "C" fn luaopen_ws_core(lua: *mut c_void) -> c_int {
        // SAFETY: Lua calls this with a valid state, which is all ws_open needs.
        unsafe { ws_open(lua) }
    }
}
