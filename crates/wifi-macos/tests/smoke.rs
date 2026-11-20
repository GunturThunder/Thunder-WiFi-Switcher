//! Smoke test for CI: CI runners have no Wi-Fi hardware and no Location
//! permission, so this only checks that every read-only call returns
//! (a value or a sensible error) instead of crashing. It never connects.

#![cfg(target_os = "macos")]

use thunder_core::{Error, Permission, WifiBackend};
use wifi_macos::MacBackend;

fn sensible(result: Result<(), Error>, permission: Permission) {
    match result {
        Ok(()) => {}
        Err(Error::PermissionDenied) => assert_ne!(permission, Permission::Granted),
        Err(Error::Os(message)) => assert!(!message.is_empty()),
        Err(other) => panic!("unexpected error from a read-only call: {other:?}"),
    }
}

#[test]
fn every_read_only_call_returns() {
    let wifi = MacBackend::new();
    let permission = wifi.permission();
    let powered = wifi.powered();

    let current = wifi.current();
    if let Ok(Some(link)) = &current {
        assert!(powered);
        assert!(link.rssi < 0, "RSSI is in dBm: {}", link.rssi);
    }
    sensible(current.map(drop), permission);

    let scan = wifi.scan();
    if let Ok(networks) = &scan {
        assert!(networks.iter().all(|network| network.rssi < 0));
    }
    sensible(scan.map(drop), permission);

    sensible(wifi.known_networks().map(drop), permission);
    sensible(wifi.gateway().map(drop), permission);
}
