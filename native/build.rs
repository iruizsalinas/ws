//! Compiles the Lua glue (ws.c) with the system C compiler when WS_GLUE=1
//! (scripts/build.lua sets it). Without it only the Rust ABI is built, so
//! `cargo check`, `clippy` and `test` need no C compiler. WS_WERROR=1, set
//! in CI, makes warnings errors; installs leave it off, so a newer compiler's
//! new warnings cannot break them.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    for path in ["ws.c", "calls.c", "values.c", "luaapi.c", "ws.h", "glue.h", "luaapi.h"] {
        println!("cargo::rerun-if-changed={path}");
    }
    for var in ["WS_GLUE", "WS_WERROR", "CC"] {
        println!("cargo::rerun-if-env-changed={var}");
    }
    println!("cargo::rustc-check-cfg=cfg(glue)");
    if env::var("WS_GLUE").as_deref() != Ok("1") {
        return;
    }
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let (windows, macos) = (os == "windows", os == "macos");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let cc = env::var("CC").unwrap_or_else(|_| if windows { "gcc" } else { "cc" }.into());
    for source in ["ws", "calls", "values", "luaapi"] {
        let object = out.join(format!("{source}.o"));
        let mut cmd = Command::new(&cc);
        cmd.args(["-c", "-O2", "-std=c99", "-Wall", "-Wextra"]);
        if env::var("WS_WERROR").as_deref() == Ok("1") {
            cmd.arg("-Werror");
        }
        if !windows {
            cmd.arg("-fPIC");
        }
        cmd.arg(format!("{source}.c")).arg("-o").arg(&object);
        let status = cmd.status().unwrap_or_else(|e| panic!("cannot run {cc}: {e}"));
        assert!(status.success(), "compiling {source}.c failed");
        println!("cargo::rustc-link-arg={}", object.display());
    }
    if macos {
        // Lua's symbols come from the host interpreter at load time.
        println!("cargo::rustc-link-arg=-undefined");
        println!("cargo::rustc-link-arg=dynamic_lookup");
    }
    if windows {
        println!("cargo::rustc-link-arg=-lkernel32");
    }
    if os == "linux" {
        // dladdr and dlopen, in libc itself only since glibc 2.34.
        println!("cargo::rustc-link-arg=-ldl");
    }
    println!("cargo::rustc-cfg=glue");
}
