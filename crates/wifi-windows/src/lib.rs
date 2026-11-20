//! Windows Wi-Fi backend for Thunder WiFi Switcher.
//!
//! Implements `thunder_core::WifiBackend` with the Native Wifi API
//! (`wlanapi.dll`) through the `windows` crate. It joins networks using the
//! profiles Windows has already saved, so the app never stores passwords here.
//!
//! The whole crate compiles only on Windows; on other platforms it is empty.
//! Implemented in Phase 2 of the roadmap.

#![cfg(windows)]
