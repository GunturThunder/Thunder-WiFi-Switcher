//! A pretend Wi-Fi backend for tests, and for working on the app without
//! several real networks (the `--simulate` mode builds on it).
//!
//! Set up the networks "in range" and their signal, then let the engine scan
//! and connect as it would on a real laptop:
//!
//! ```
//! use thunder_core::fake::FakeBackend;
//! use thunder_core::WifiBackend;
//!
//! let wifi = FakeBackend::new();
//! wifi.set_network("Dorm-5G", -62);
//! wifi.set_network("Dorm-2.4", -70);
//! wifi.join("Dorm-5G");
//! assert_eq!(wifi.current().unwrap().unwrap().rssi, -62);
//! ```

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::wifi::{Error, Link, Permission, Result, ScanResult, WifiBackend};

/// A call made to the fake, for checking what the engine did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// [`WifiBackend::scan`] was called.
    Scan,
    /// [`WifiBackend::connect`] was called for this network.
    Connect(String),
}

/// Pretend Wi-Fi hardware. All methods take `&self` so tests can change the
/// "room" while the engine holds the backend.
#[derive(Debug)]
pub struct FakeBackend {
    state: Mutex<FakeState>,
}

#[derive(Debug)]
struct FakeState {
    visible: Vec<ScanResult>,
    known: BTreeSet<String>,
    connected: Option<String>,
    powered: bool,
    permission: Permission,
    gateway: IpAddr,
    failures: HashMap<String, Error>,
    calls: Vec<Call>,
    /// Per network: public-server ping (`None` = internet down).
    internet: HashMap<String, Option<u32>>,
    /// Per network: router ping (`None` = router silent).
    router: HashMap<String, Option<u32>>,
    /// Networks that drop ping (ICMP) but allow TCP.
    icmp_blocked: HashSet<String>,
    /// Public servers that are down everywhere.
    servers_down: HashSet<IpAddr>,
}

/// Ping on a network whose internet works, unless set otherwise.
const DEFAULT_PING_MS: u32 = 20;
/// Router ping, unless set otherwise.
const DEFAULT_ROUTER_MS: u32 = 3;

impl Default for FakeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeBackend {
    /// Wi-Fi on, permission granted, no networks in range.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(FakeState {
                visible: Vec::new(),
                known: BTreeSet::new(),
                connected: None,
                powered: true,
                permission: Permission::Granted,
                gateway: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
                failures: HashMap::new(),
                calls: Vec::new(),
                internet: HashMap::new(),
                router: HashMap::new(),
                icmp_blocked: HashSet::new(),
                servers_down: HashSet::new(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts a saved network in range (or changes its signal). Each network
    /// has one access point.
    pub fn set_network(&self, ssid: &str, rssi: i32) {
        let mut state = self.state();
        state.known.insert(ssid.to_owned());
        match state.visible.iter_mut().find(|result| result.ssid == ssid) {
            Some(result) => result.rssi = rssi,
            None => state.visible.push(ScanResult {
                ssid: ssid.to_owned(),
                bssid: Some(format!("fake:{ssid}")),
                rssi,
                channel: Some(36),
            }),
        }
    }

    /// Takes a network out of range. If the laptop was on it, the connection
    /// drops.
    pub fn remove_network(&self, ssid: &str) {
        let mut state = self.state();
        state.visible.retain(|result| result.ssid != ssid);
        if state.connected.as_deref() == Some(ssid) {
            state.connected = None;
        }
    }

    /// Forgets a network's password, so connecting to it fails.
    pub fn forget(&self, ssid: &str) {
        self.state().known.remove(ssid);
    }

    /// Connects as if the OS (or the user) did it, without recording a call.
    pub fn join(&self, ssid: &str) {
        self.state().connected = Some(ssid.to_owned());
    }

    /// Drops the connection.
    pub fn disconnect(&self) {
        self.state().connected = None;
    }

    /// Turns the Wi-Fi radio on or off. Turning it off disconnects.
    pub fn set_powered(&self, on: bool) {
        let mut state = self.state();
        state.powered = on;
        if !on {
            state.connected = None;
        }
    }

    /// Sets the Location permission state.
    pub fn set_permission(&self, permission: Permission) {
        self.state().permission = permission;
    }

    /// Makes every connection attempt to `ssid` fail with `error`.
    pub fn fail_connect(&self, ssid: &str, error: Error) {
        self.state().failures.insert(ssid.to_owned(), error);
    }

    /// Lets connections to `ssid` work again.
    pub fn allow_connect(&self, ssid: &str) {
        self.state().failures.remove(ssid);
    }

    /// The network it's connected to, if any.
    pub fn connected(&self) -> Option<String> {
        self.state().connected.clone()
    }

    /// Every scan and connect call so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.state().calls.clone()
    }

    /// Sets the ping to public servers on a network; `None` means its
    /// internet is down. Defaults to 20 ms.
    pub fn set_internet(&self, ssid: &str, ping_ms: Option<u32>) {
        self.state().internet.insert(ssid.to_owned(), ping_ms);
    }

    /// Sets the router's ping on a network; `None` means it doesn't answer.
    /// Defaults to 3 ms.
    pub fn set_router(&self, ssid: &str, ping_ms: Option<u32>) {
        self.state().router.insert(ssid.to_owned(), ping_ms);
    }

    /// Makes a network drop ping (ICMP) while TCP still works.
    pub fn block_icmp(&self, ssid: &str) {
        self.state().icmp_blocked.insert(ssid.to_owned());
    }

    /// Takes one public server down (or back up) on every network.
    pub fn set_server_down(&self, addr: IpAddr, down: bool) {
        let mut state = self.state();
        if down {
            state.servers_down.insert(addr);
        } else {
            state.servers_down.remove(&addr);
        }
    }

    /// The simulated round trip to `addr` on the current network.
    fn round_trip(&self, addr: IpAddr, icmp: bool) -> Result<Duration> {
        let state = self.state();
        let Some(ssid) = state.connected.clone() else {
            return Err(Error::Os("not connected".into()));
        };
        if icmp && state.icmp_blocked.contains(&ssid) {
            return Err(Error::Timeout);
        }
        let ping = if addr == state.gateway {
            state
                .router
                .get(&ssid)
                .copied()
                .unwrap_or(Some(DEFAULT_ROUTER_MS))
        } else if state.servers_down.contains(&addr) {
            None
        } else {
            state
                .internet
                .get(&ssid)
                .copied()
                .unwrap_or(Some(DEFAULT_PING_MS))
        };
        ping.map(|ms| Duration::from_millis(ms.into()))
            .ok_or(Error::Timeout)
    }
}

impl WifiBackend for FakeBackend {
    fn current(&self) -> Result<Option<Link>> {
        let state = self.state();
        if state.permission != Permission::Granted {
            return Err(Error::PermissionDenied);
        }
        let Some(ssid) = state.connected.as_ref() else {
            return Ok(None);
        };
        Ok(state
            .visible
            .iter()
            .find(|result| &result.ssid == ssid)
            .map(|result| Link {
                ssid: result.ssid.clone(),
                bssid: result.bssid.clone(),
                rssi: result.rssi,
                channel: result.channel,
            }))
    }

    fn scan(&self) -> Result<Vec<ScanResult>> {
        let mut state = self.state();
        state.calls.push(Call::Scan);
        if state.permission != Permission::Granted {
            return Err(Error::PermissionDenied);
        }
        if !state.powered {
            return Ok(Vec::new());
        }
        Ok(state.visible.clone())
    }

    fn connect(&self, ssid: &str) -> Result<()> {
        let mut state = self.state();
        state.calls.push(Call::Connect(ssid.to_owned()));
        if let Some(error) = state.failures.get(ssid) {
            return Err(error.clone());
        }
        if !state.powered || !state.visible.iter().any(|result| result.ssid == ssid) {
            return Err(Error::NotFound);
        }
        if !state.known.contains(ssid) {
            return Err(Error::AuthFailed);
        }
        state.connected = Some(ssid.to_owned());
        Ok(())
    }

    fn known_networks(&self) -> Result<Vec<String>> {
        Ok(self.state().known.iter().cloned().collect())
    }

    fn gateway(&self) -> Result<Option<IpAddr>> {
        let state = self.state();
        Ok(state.connected.as_ref().map(|_| state.gateway))
    }

    fn permission(&self) -> Permission {
        self.state().permission
    }

    fn request_permission(&self) {
        let mut state = self.state();
        if state.permission == Permission::NotDetermined {
            state.permission = Permission::Granted;
        }
    }

    fn powered(&self) -> bool {
        self.state().powered
    }

    fn ping(&self, addr: IpAddr, _timeout: Duration) -> Result<Duration> {
        self.round_trip(addr, true)
    }

    fn tcp_probe(&self, addr: IpAddr, _timeout: Duration) -> Result<Duration> {
        self.round_trip(addr, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connects_only_to_saved_networks_in_range() {
        let wifi = FakeBackend::new();
        wifi.set_network("A", -60);
        wifi.set_network("B", -70);
        wifi.forget("B");

        assert_eq!(wifi.connect("Nowhere"), Err(Error::NotFound));
        assert_eq!(wifi.connect("B"), Err(Error::AuthFailed));
        assert_eq!(wifi.connect("A"), Ok(()));
        assert_eq!(wifi.connected().as_deref(), Some("A"));
        assert_eq!(
            wifi.calls(),
            [
                Call::Connect("Nowhere".into()),
                Call::Connect("B".into()),
                Call::Connect("A".into())
            ]
        );
    }

    #[test]
    fn going_out_of_range_drops_the_connection() {
        let wifi = FakeBackend::new();
        wifi.set_network("A", -60);
        wifi.join("A");
        assert_eq!(wifi.current().unwrap().map(|link| link.rssi), Some(-60));
        wifi.remove_network("A");
        assert_eq!(wifi.current().unwrap(), None);
        assert_eq!(wifi.gateway().unwrap(), None);
    }

    #[test]
    fn scripted_failures() {
        let wifi = FakeBackend::new();
        wifi.set_network("A", -60);
        wifi.fail_connect("A", Error::Timeout);
        assert_eq!(wifi.connect("A"), Err(Error::Timeout));
        wifi.allow_connect("A");
        assert_eq!(wifi.connect("A"), Ok(()));
    }

    #[test]
    fn needs_permission_to_read_networks() {
        let wifi = FakeBackend::new();
        wifi.set_permission(Permission::NotDetermined);
        assert_eq!(wifi.current(), Err(Error::PermissionDenied));
        wifi.request_permission();
        assert_eq!(wifi.permission(), Permission::Granted);
        assert_eq!(wifi.current(), Ok(None));
    }

    #[test]
    fn radio_off_disconnects_and_sees_nothing() {
        let wifi = FakeBackend::new();
        wifi.set_network("A", -60);
        wifi.join("A");
        wifi.set_powered(false);
        assert!(!wifi.powered());
        assert_eq!(wifi.connected(), None);
        assert_eq!(wifi.scan(), Ok(Vec::new()));
    }
}
