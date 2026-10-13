//! Compiles the Lua glue (ws.c) with the system C compiler.
//!
//! LUA_INCDIR points at the Lua headers; on Windows LUA_LIB names the Lua
//! DLL (or its import library) to link against. Without LUA_INCDIR only the
//! ABI is built, which keeps `cargo check` and `cargo clippy` working.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    for path in ["ws.c", "ws.h"] {
        println!("cargo::rerun-if-changed={path}");
    }
    for var in ["LUA_INCDIR", "LUA_LIB", "CC"] {
        println!("cargo::rerun-if-env-changed={var}");
    }
    println!("cargo::rustc-check-cfg=cfg(glue)");
    let Ok(include) = env::var("LUA_INCDIR") else {
        println!("cargo::warning=LUA_INCDIR is not set; building without the Lua glue");
        return;
    };
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let macos = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    let object = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("ws.o");
    let cc = env::var("CC").unwrap_or_else(|_| if windows { "gcc" } else { "cc" }.into());
    let mut cmd = Command::new(&cc);
    cmd.args(["-c", "-O2", "-std=c99", "-Wall", "-Wextra", "-Werror"]);
    if !windows {
        cmd.arg("-fPIC");
    }
    cmd.arg(format!("-I{include}")).arg("ws.c").arg("-o").arg(&object);
    let status = cmd.status().unwrap_or_else(|e| panic!("cannot run {cc}: {e}"));
    assert!(status.success(), "compiling ws.c failed");
    println!("cargo::rustc-link-arg={}", object.display());
    if macos {
        // Lua's symbols come from the host interpreter at load time.
        println!("cargo::rustc-link-arg=-undefined");
        println!("cargo::rustc-link-arg=dynamic_lookup");
    }
    if windows {
        let lib = env::var("LUA_LIB").expect("LUA_LIB must name the Lua DLL on Windows");
        println!("cargo::rustc-link-arg={lib}");
    }
    println!("cargo::rustc-cfg=glue");
}
