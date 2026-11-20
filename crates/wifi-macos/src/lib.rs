//! macOS Wi-Fi backend for Thunder WiFi Switcher.
//!
//! [`MacBackend`] implements `thunder_core::WifiBackend` with Apple's CoreWLAN
//! framework. Network passwords live in the Keychain under the service
//! `ThunderWiFi` ([`PasswordStore`]), because macOS doesn't let apps use the
//! passwords it has saved itself. The Swift prototype in `prototype/macos` is
//! the reference for each call.
//!
//! Two things need the app to be running a main run loop (the Tauri app always
//! is; the `probe` example runs one itself):
//! - [`MacBackend::request_permission`](thunder_core::WifiBackend::request_permission)
//!   shows the Location prompt from the main thread.
//! - [`SleepWatcher`] receives sleep and wake notifications on the main thread.
//!
//! The whole crate compiles only on macOS; on other platforms it is empty.

#![cfg(target_os = "macos")]

mod backend;
mod gateway;
mod icmp;
mod keychain;
mod location;
mod power;

pub use backend::MacBackend;
pub use keychain::PasswordStore;
pub use power::{PowerEvent, SleepWatcher};
