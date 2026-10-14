//! wss:// with a private CA.

use super::common::*;
use ws_core::*;

#[path = "tls/activity.rs"]
mod activity;
#[path = "tls/basic.rs"]
mod basic;
#[path = "tls/close.rs"]
mod close;
#[path = "tls/control.rs"]
mod control;
#[path = "tls/flood.rs"]
mod flood;
#[path = "tls/pong.rs"]
mod pong;
