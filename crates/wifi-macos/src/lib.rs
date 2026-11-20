//! macOS Wi-Fi backend for Thunder WiFi Switcher.
//!
//! Implements `thunder_core::WifiBackend` with Apple's CoreWLAN framework
//! (through `objc2-core-wlan`) and stores network passwords in the Keychain
//! under the service `ThunderWiFi`. The Swift prototype in `prototype/macos`
//! is the reference for each call.
//!
//! The whole crate compiles only on macOS; on other platforms it is empty.
//! Implemented in Phase 2 of the roadmap.

#![cfg(target_os = "macos")]
