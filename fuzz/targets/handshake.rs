//! Handshake and URL parsing never panic on arbitrary input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ws_core::proto::{handshake, url};

fuzz_target!(|data: &[u8]| {
    let _ = handshake::request(data);
    let _ = handshake::response(data, "dGhlIHNhbXBsZSBub25jZQ==", &["chat".to_string()]);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = url::parse(text);
    }
});
