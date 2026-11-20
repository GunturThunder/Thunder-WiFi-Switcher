//! What the core knows about Wi-Fi hardware: the types the platform backends
//! return, and the [`WifiBackend`] trait they implement.

use std::net::IpAddr;
use std::sync::Arc;

/// The network the laptop is connected to right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Network name.
    pub ssid: String,
    /// Access point address, if the OS reports it.
    pub bssid: Option<String>,
    /// Signal strength in dBm, e.g. `-58`. Always dBm on every OS; the Windows
    /// backend reads the BSS list's `lRssi` rather than its 0–100% quality.
    pub rssi: i32,
    /// Wi-Fi channel, if the OS reports it.
    pub channel: Option<u32>,
}

/// One access point seen in a scan. A network with several access points
/// (one SSID, several BSSIDs) appears once per access point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    /// Network name. Empty for hidden networks.
    pub ssid: String,
    /// Access point address, if the OS reports it.
    pub bssid: Option<String>,
    /// Signal strength in dBm.
    pub rssi: i32,
    /// Wi-Fi channel, if the OS reports it.
    pub channel: Option<u32>,
}

/// Location permission state. Both macOS and Windows 11 24H2+ hide network
/// names from apps without it (see PRIVACY.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// The app can read network names.
    Granted,
    /// The user said no, or turned it off in System Settings.
    Denied,
    /// The user hasn't been asked yet.
    NotDetermined,
}

/// Everything a backend call can fail with, phrased so the UI can show it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The OS won't share network names without Location permission.
    #[error("Location permission is needed to see Wi-Fi network names")]
    PermissionDenied,
    /// The network isn't in range, or isn't saved on this computer.
    #[error("the network wasn't found")]
    NotFound,
    /// Wrong password, or the network rejected the connection.
    #[error("the network rejected the connection (wrong password?)")]
    AuthFailed,
    /// The OS didn't answer in time.
    #[error("Wi-Fi didn't respond in time")]
    Timeout,
    /// Any other OS error, with the OS's own message.
    #[error("Wi-Fi error: {0}")]
    Os(String),
}

/// Result type for backend calls.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The only way the core talks to Wi-Fi. Each OS crate implements it, and
/// tests use [`crate::fake::FakeBackend`].
pub trait WifiBackend: Send + Sync {
    /// Current link, or `None` if disconnected.
    fn current(&self) -> Result<Option<Link>>;
    /// Active scan. Blocking, so call it off the main thread. Returns every
    /// visible access point.
    fn scan(&self) -> Result<Vec<ScanResult>>;
    /// Join a network. The password comes from the backend's own store
    /// (macOS Keychain, or the profile Windows has saved).
    fn connect(&self, ssid: &str) -> Result<()>;
    /// Networks the OS (or our store) can join without asking the user.
    fn known_networks(&self) -> Result<Vec<String>>;
    /// Router (gateway) address for the current link, for the local ping.
    fn gateway(&self) -> Result<Option<IpAddr>>;
    /// Location permission state.
    fn permission(&self) -> Permission;
    /// Ask the user for Location permission (shows the OS prompt if allowed).
    fn request_permission(&self);
    /// Whether the Wi-Fi radio is on.
    fn powered(&self) -> bool;
}

/// Lets the app share one backend between the worker thread and the UI.
impl<T: WifiBackend + ?Sized> WifiBackend for Arc<T> {
    fn current(&self) -> Result<Option<Link>> {
        (**self).current()
    }
    fn scan(&self) -> Result<Vec<ScanResult>> {
        (**self).scan()
    }
    fn connect(&self, ssid: &str) -> Result<()> {
        (**self).connect(ssid)
    }
    fn known_networks(&self) -> Result<Vec<String>> {
        (**self).known_networks()
    }
    fn gateway(&self) -> Result<Option<IpAddr>> {
        (**self).gateway()
    }
    fn permission(&self) -> Permission {
        (**self).permission()
    }
    fn request_permission(&self) {
        (**self).request_permission()
    }
    fn powered(&self) -> bool {
        (**self).powered()
    }
}
