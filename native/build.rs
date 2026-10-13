//! Compiles the Lua glue (ws.c) with the system C compiler when WS_GLUE=1
//! (scripts/build.lua sets it). Without it only the Rust ABI is built, so
//! `cargo check`, `clippy` and `test` need no C compiler.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    for path in ["ws.c", "ws.h", "luaapi.h"] {
        println!("cargo::rerun-if-changed={path}");
    }
    for var in ["WS_GLUE", "CC"] {
        println!("cargo::rerun-if-env-changed={var}");
    }
    println!("cargo::rustc-check-cfg=cfg(glue)");
    if env::var("WS_GLUE").as_deref() != Ok("1") {
        return;
    }
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let macos = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    let object = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("ws.o");
    let cc = env::var("CC").unwrap_or_else(|_| if windows { "gcc" } else { "cc" }.into());
    let mut cmd = Command::new(&cc);
    cmd.args(["-c", "-O2", "-std=c99", "-Wall", "-Wextra", "-Werror"]);
    if !windows {
        cmd.arg("-fPIC");
    }
    cmd.arg("ws.c").arg("-o").arg(&object);
    let status = cmd.status().unwrap_or_else(|e| panic!("cannot run {cc}: {e}"));
    assert!(status.success(), "compiling ws.c failed");
    println!("cargo::rustc-link-arg={}", object.display());
    if macos {
        // Lua's symbols come from the host interpreter at load time.
        println!("cargo::rustc-link-arg=-undefined");
        println!("cargo::rustc-link-arg=dynamic_lookup");
    }
    if windows {
        println!("cargo::rustc-link-arg=-lkernel32");
    }
    println!("cargo::rustc-cfg=glue");
}
