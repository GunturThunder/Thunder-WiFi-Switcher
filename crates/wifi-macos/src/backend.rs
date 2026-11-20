//! CoreWLAN: current link, scan, connect, known networks.

use std::collections::BTreeSet;
use std::net::IpAddr;

use objc2::rc::Retained;
use objc2_core_wlan::{CWErr, CWInterface, CWNetwork, CWWiFiClient};
use objc2_foundation::{NSError, NSString};
use thunder_core::{Error, Link, Permission, Result, ScanResult, WifiBackend};

use crate::keychain::PasswordStore;
use crate::{gateway, location};

/// The macOS backend. Cheap to create and share: it holds no CoreWLAN
/// objects, it asks for the Wi-Fi interface on each call.
#[derive(Debug, Clone, Default)]
pub struct MacBackend {
    passwords: PasswordStore,
}

impl MacBackend {
    /// A backend that reads passwords from the `ThunderWiFi` Keychain service.
    pub fn new() -> Self {
        Self::default()
    }

    /// A backend with a different password store (for tests).
    pub fn with_passwords(passwords: PasswordStore) -> Self {
        Self { passwords }
    }

    /// The password store this backend uses.
    pub fn passwords(&self) -> &PasswordStore {
        &self.passwords
    }
}

fn interface() -> Result<Retained<CWInterface>> {
    // SAFETY: plain CoreWLAN getters with no preconditions.
    unsafe { CWWiFiClient::sharedWiFiClient().interface() }
        .ok_or_else(|| Error::Os("this Mac has no Wi-Fi interface".into()))
}

fn string(value: Option<Retained<NSString>>) -> Option<String> {
    value.map(|s| s.to_string())
}

fn scan_result(network: &CWNetwork) -> ScanResult {
    // SAFETY: plain CWNetwork getters.
    unsafe {
        ScanResult {
            ssid: string(network.ssid()).unwrap_or_default(),
            bssid: string(network.bssid()),
            rssi: network.rssiValue() as i32,
            channel: network
                .wlanChannel()
                .map(|channel| channel.channelNumber() as u32),
        }
    }
}

impl WifiBackend for MacBackend {
    fn current(&self) -> Result<Option<Link>> {
        let iface = interface()?;
        // SAFETY: plain CWInterface getters.
        let (powered, ssid, bssid, rssi, channel) = unsafe {
            (
                iface.powerOn(),
                string(iface.ssid()),
                string(iface.bssid()),
                iface.rssiValue() as i32,
                iface.wlanChannel().map(|c| c.channelNumber() as u32),
            )
        };
        if !powered {
            return Ok(None);
        }
        // RSSI is 0 when not associated with any network.
        if rssi == 0 {
            return Ok(None);
        }
        if ssid.is_none() && location::status() != Permission::Granted {
            // Connected, but macOS hides the name without Location permission.
            return Err(Error::PermissionDenied);
        }
        Ok(Some(Link {
            // A hidden network with permission granted: no name to report.
            ssid: ssid.unwrap_or_default(),
            bssid,
            rssi,
            channel,
        }))
    }

    fn scan(&self) -> Result<Vec<ScanResult>> {
        if location::status() != Permission::Granted {
            // Scans still work, but every network comes back without a name.
            return Err(Error::PermissionDenied);
        }
        let iface = interface()?;
        // SAFETY: blocking scan; this is called off the main thread.
        let networks =
            unsafe { iface.scanForNetworksWithName_error(None) }.map_err(|err| map_error(&err))?;
        Ok(networks
            .iter()
            .map(|network| scan_result(&network))
            .collect())
    }

    fn connect(&self, ssid: &str) -> Result<()> {
        let iface = interface()?;
        let name = NSString::from_str(ssid);
        // SAFETY: blocking scan for one network name.
        let found = unsafe { iface.scanForNetworksWithName_error(Some(&name)) }
            .map_err(|err| map_error(&err))?;
        // The strongest access point with exactly this name.
        let target = found
            .iter()
            .filter(|network| string(unsafe { network.ssid() }).as_deref() == Some(ssid))
            .max_by_key(|network| unsafe { network.rssiValue() })
            .ok_or(Error::NotFound)?;

        // No stored password: try without one (open networks).
        let password = self.passwords.get(ssid)?.map(|p| NSString::from_str(&p));
        // SAFETY: blocks until associated or failed; called off the main thread.
        unsafe { iface.associateToNetwork_password_error(&target, password.as_deref()) }
            .map_err(|err| map_error(&err))
    }

    fn known_networks(&self) -> Result<Vec<String>> {
        let mut names: BTreeSet<String> = self.passwords.saved_networks()?.into_iter().collect();
        if let Ok(iface) = interface() {
            // Networks in macOS's own "Known Networks" list.
            // SAFETY: plain getters.
            if let Some(config) = unsafe { iface.configuration() } {
                let profiles = unsafe { config.networkProfiles() };
                names.extend(
                    profiles
                        .array()
                        .iter()
                        .filter_map(|profile| string(unsafe { profile.ssid() })),
                );
            }
        }
        Ok(names.into_iter().collect())
    }

    fn gateway(&self) -> Result<Option<IpAddr>> {
        let iface = interface()?;
        // SAFETY: plain getter.
        let name = string(unsafe { iface.interfaceName() });
        Ok(name.and_then(|name| gateway::router_for(&name)))
    }

    fn permission(&self) -> Permission {
        location::status()
    }

    fn request_permission(&self) {
        location::request();
    }

    fn powered(&self) -> bool {
        // SAFETY: plain getter.
        interface().is_ok_and(|iface| unsafe { iface.powerOn() })
    }
}

/// Turns a CoreWLAN error into the core's error type.
fn map_error(err: &NSError) -> Error {
    let domain = err.domain().to_string();
    let message = err.localizedDescription().to_string();
    classify(&domain, err.code(), message)
}

/// Pure part of [`map_error`], so it can be tested without hardware.
fn classify(domain: &str, code: isize, message: String) -> Error {
    if domain != "com.apple.coreWLAN.error" && domain != "CWErrorDomain" {
        return Error::Os(message);
    }
    match CWErr(code) {
        CWErr::CWTimeoutErr | CWErr::CWSupplicantTimeoutErr => Error::Timeout,
        // Wrong password, or the access point refused us.
        CWErr::CWEAPOLErr
        | CWErr::CWAssociationDeniedErr
        | CWErr::CWReassociationDeniedErr
        | CWErr::CWChallengeFailureErr
        | CWErr::CWInvalidPMKErr => Error::AuthFailed,
        _ => Error::Os(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_errors_become_auth_failed() {
        for code in [1, -3909, -3908, -3912, -3924] {
            assert_eq!(
                classify("com.apple.coreWLAN.error", code, "x".into()),
                Error::AuthFailed,
                "code {code}"
            );
        }
    }

    #[test]
    fn timeouts_become_timeout() {
        assert_eq!(
            classify("com.apple.coreWLAN.error", -3905, "x".into()),
            Error::Timeout
        );
        assert_eq!(
            classify("com.apple.coreWLAN.error", -3925, "x".into()),
            Error::Timeout
        );
    }

    #[test]
    fn everything_else_keeps_the_os_message() {
        assert_eq!(
            classify("com.apple.coreWLAN.error", -3931, "Operation failed".into()),
            Error::Os("Operation failed".into())
        );
        assert_eq!(
            classify("NSPOSIXErrorDomain", 1, "Not permitted".into()),
            Error::Os("Not permitted".into())
        );
    }
}
