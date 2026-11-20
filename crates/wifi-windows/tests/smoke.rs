//! Smoke test for CI: Windows runners have no Wi-Fi adapter (and usually no
//! WLAN AutoConfig service), so this only checks that every read-only call
//! returns a value or a sensible error instead of crashing. It never connects.

#![cfg(windows)]

use thunder_core::{Error, WifiBackend};
use wifi_windows::{PowerWatcher, WinBackend};

fn sensible(result: Result<(), Error>) {
    match result {
        Ok(()) | Err(Error::PermissionDenied) => {}
        Err(Error::Os(message)) => assert!(!message.is_empty()),
        Err(other) => panic!("unexpected error from a read-only call: {other:?}"),
    }
}

#[test]
fn every_read_only_call_returns() {
    let wifi = WinBackend::new();
    let _ = wifi.powered();
    let _ = wifi.permission();

    let current = wifi.current();
    if let Ok(Some(link)) = &current {
        assert!(link.rssi < 0, "RSSI is in dBm: {}", link.rssi);
    }
    sensible(current.map(drop));
    sensible(wifi.scan().map(drop));
    sensible(wifi.known_networks().map(drop));
    sensible(wifi.gateway().map(drop));
}

#[test]
fn power_watcher_registers_and_unregisters() {
    let watcher = PowerWatcher::new(|_| {});
    // Registering works on any Windows machine; dropping must not crash.
    assert!(watcher.is_ok(), "{watcher:?}");
    drop(watcher);
}
