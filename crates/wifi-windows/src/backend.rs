//! `WifiBackend` for Windows, on top of [`crate::wlan`].

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

use thunder_core::{Error, Link, Permission, Result, ScanResult, WifiBackend};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::gateway;
use crate::wlan::{Session, bssid_text, quality_to_dbm};

/// Windows drivers must finish a scan within 4 seconds.
const SCAN_TIMEOUT: Duration = Duration::from_secs(5);
/// Joining (including the WPA handshake) normally takes a few seconds.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// The Windows backend. Holds nothing: each call opens its own WLAN session,
/// so it's cheap to share between threads.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinBackend;

impl WinBackend {
    /// A new backend.
    pub fn new() -> Self {
        Self
    }
}

impl WifiBackend for WinBackend {
    fn current(&self) -> Result<Option<Link>> {
        let session = Session::open()?;
        let interface = session.primary_interface()?;
        if !session.radio_on(&interface) {
            return Ok(None);
        }
        let Some(connection) = session.current(&interface)? else {
            return Ok(None);
        };
        // The connection reports 0–100% quality; the BSS list has real dBm.
        let bss = session
            .bss_list(&interface)
            .ok()
            .and_then(|list| list.into_iter().find(|b| b.bssid == connection.bssid));
        Ok(Some(Link {
            ssid: connection.ssid,
            bssid: Some(bssid_text(&connection.bssid)),
            rssi: bss
                .as_ref()
                .map_or_else(|| quality_to_dbm(connection.quality), |b| b.rssi),
            channel: bss.and_then(|b| b.channel),
        }))
    }

    fn scan(&self) -> Result<Vec<ScanResult>> {
        let session = Session::open()?;
        let interface = session.primary_interface()?;
        session.scan_and_wait(&interface, SCAN_TIMEOUT)?;
        Ok(session
            .bss_list(&interface)?
            .into_iter()
            .map(|bss| ScanResult {
                ssid: bss.ssid,
                bssid: Some(bssid_text(&bss.bssid)),
                rssi: bss.rssi,
                channel: bss.channel,
            })
            .collect())
    }

    fn connect(&self, ssid: &str) -> Result<()> {
        let session = Session::open()?;
        let interface = session.primary_interface()?;
        let available = session.available(&interface)?;
        let profile = available
            .iter()
            .filter(|network| network.ssid == ssid)
            .find_map(|network| network.profile.clone());
        match profile {
            Some(profile) => session.connect_and_wait(&interface, &profile, CONNECT_TIMEOUT),
            None if available.iter().any(|network| network.ssid == ssid) => {
                Err(Error::Os(format!(
                    "Windows hasn't saved a password for {ssid}. Connect to it once from the Windows Wi-Fi menu first."
                )))
            }
            None => Err(Error::NotFound),
        }
    }

    fn known_networks(&self) -> Result<Vec<String>> {
        let session = Session::open()?;
        let interface = session.primary_interface()?;
        // Profile names are the network names unless someone renamed them.
        let names: BTreeSet<String> = session.profiles(&interface)?.into_iter().collect();
        Ok(names.into_iter().collect())
    }

    fn gateway(&self) -> Result<Option<IpAddr>> {
        let session = Session::open()?;
        let interface = session.primary_interface()?;
        Ok(gateway::router_for(&interface))
    }

    fn permission(&self) -> Permission {
        // Windows 11 24H2+ answers "access denied" when location is off for
        // desktop apps. Anything else (including no adapter) means the app
        // isn't being blocked.
        let blocked = Session::open()
            .and_then(|session| {
                let interface = session.primary_interface()?;
                session.bss_list(&interface).map(drop)
            })
            .is_err_and(|err| err == Error::PermissionDenied);
        if blocked {
            Permission::Denied
        } else {
            Permission::Granted
        }
    }

    fn request_permission(&self) {
        // Desktop apps can't show a prompt themselves; open the setting.
        // SAFETY: plain call with static strings.
        unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                w!("ms-settings:privacy-location"),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    fn powered(&self) -> bool {
        Session::open()
            .and_then(|session| {
                let interface = session.primary_interface()?;
                Ok(session.radio_on(&interface))
            })
            .unwrap_or(false)
    }
}
