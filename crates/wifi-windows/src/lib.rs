//! Windows Wi-Fi backend for Thunder WiFi Switcher.
//!
//! [`WinBackend`] implements `thunder_core::WifiBackend` with the Native Wifi
//! API (`wlanapi.dll`) through the official `windows` crate. It joins networks
//! with the profiles Windows has already saved, so the app never stores
//! passwords on Windows. [`PowerWatcher`] reports sleep and resume.
//!
//! On Windows 11 24H2 and later, Wi-Fi details count as location data: when
//! location is off for desktop apps, wlanapi answers "access denied", which
//! the backend reports as `Error::PermissionDenied`.
//!
//! The whole crate compiles only on Windows; on other platforms it is empty.

#![cfg(windows)]

mod backend;
mod gateway;
mod power;
mod wlan;

pub use backend::WinBackend;
pub use power::{PowerEvent, PowerWatcher};
