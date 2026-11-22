//! CoreWLAN: current link, scan, connect, known networks.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::thread;
use std::time::Duration;

use objc2::rc::Retained;
use objc2_core_wlan::{CWErr, CWInterface, CWNetwork, CWSecurity, CWWiFiClient};
use objc2_foundation::{NSError, NSString};
use thunder_core::{Error, Link, Permission, Result, ScanResult, WifiBackend};

use crate::keychain::PasswordStore;
use crate::{gateway, icmp, location};

/// The macOS backend. Cheap to share: it holds no CoreWLAN objects, it asks
/// for the Wi-Fi interface on each call.
///
/// Creating one starts a location manager on the main thread. macOS reports
/// the Location permission state through it, so
/// [`permission`](WifiBackend::permission) reads "not determined" until the
/// main run loop has run for a moment.
#[derive(Debug, Clone)]
pub struct MacBackend {
    passwords: PasswordStore,
}

impl Default for MacBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MacBackend {
    /// A backend that reads passwords from the `ThunderWiFi` Keychain service.
    pub fn new() -> Self {
        Self::with_passwords(PasswordStore::default())
    }

    /// A backend with a different password store (for tests).
    pub fn with_passwords(passwords: PasswordStore) -> Self {
        location::start();
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
        // Connected but nameless: macOS hides names without Location
        // permission. (Judged from what CoreWLAN returns rather than the
        // permission status, which macOS reports late.)
        let Some(ssid) = ssid else {
            return Err(Error::PermissionDenied);
        };
        Ok(Some(Link {
            ssid,
            bssid,
            rssi,
            channel,
        }))
    }

    fn scan(&self) -> Result<Vec<ScanResult>> {
        let iface = interface()?;
        // SAFETY: blocking scan; this is called off the main thread.
        let networks = retry_if_busy(|| unsafe { iface.scanForNetworksWithName_error(None) })?;
        let results: Vec<ScanResult> = networks
            .iter()
            .map(|network| scan_result(&network))
            .collect();
        // Without permission the scan still works, but every name is missing.
        if !results.is_empty() && results.iter().all(|r| r.ssid.is_empty()) {
            return Err(Error::PermissionDenied);
        }
        Ok(results)
    }

    fn connect(&self, ssid: &str) -> Result<()> {
        let iface = interface()?;
        let name = NSString::from_str(ssid);
        // SAFETY: blocking scan for one network name.
        let found = retry_if_busy(|| unsafe { iface.scanForNetworksWithName_error(Some(&name)) })?;
        // The strongest access point with exactly this name.
        // SAFETY (here and below): plain CWNetwork getters.
        let Some(target) = found
            .iter()
            .filter(|network| string(unsafe { network.ssid() }).as_deref() == Some(ssid))
            .max_by_key(|network| unsafe { network.rssiValue() })
        else {
            // Without Location permission the scan works, but every name is
            // missing (the same rule as `scan`).
            let any = found.iter().next().is_some();
            let nameless = found
                .iter()
                .all(|network| unsafe { network.ssid() }.is_none());
            return Err(if any && nameless {
                Error::PermissionDenied
            } else {
                Error::NotFound
            });
        };

        let security = Security::of(|kind| unsafe { target.supportsSecurity(kind) });
        let password = self.passwords.get(ssid)?.map(|p| NSString::from_str(&p));
        // SAFETY: blocks until associated or failed; called off the main thread.
        let associate = || {
            retry_if_busy(|| unsafe {
                iface.associateToNetwork_password_error(&target, password.as_deref())
            })
        };
        match (security, &password) {
            (Security::UserName, _) => Err(Error::NeedsUserName),
            // macOS might still join it with a password it knows itself; if
            // not, the missing password is the problem, not the network.
            (Security::Password, None) => associate().map_err(|err| match err {
                Error::AuthFailed | Error::Os(_) => Error::NoPassword,
                other => other,
            }),
            _ => associate(),
        }
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

    fn ping(&self, addr: IpAddr, timeout: Duration) -> Result<Duration> {
        icmp::ping(addr, timeout)
    }
}

/// What a network asks for when joining.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Security {
    /// Nothing (open networks, including Enhanced Open).
    Open,
    /// A password (WEP, or WPA, WPA2 or WPA3 Personal).
    Password,
    /// A user name and a password (WPA Enterprise, 802.1X).
    UserName,
}

impl Security {
    const OPEN: [CWSecurity; 3] = [CWSecurity::None, CWSecurity::OWE, CWSecurity::OWETransition];
    const PASSWORD: [CWSecurity; 7] = [
        CWSecurity::WEP,
        CWSecurity::WPAPersonal,
        CWSecurity::WPAPersonalMixed,
        CWSecurity::WPA2Personal,
        CWSecurity::Personal,
        CWSecurity::WPA3Personal,
        CWSecurity::WPA3Transition,
    ];
    const USER_NAME: [CWSecurity; 6] = [
        CWSecurity::DynamicWEP,
        CWSecurity::WPAEnterprise,
        CWSecurity::WPAEnterpriseMixed,
        CWSecurity::WPA2Enterprise,
        CWSecurity::Enterprise,
        CWSecurity::WPA3Enterprise,
    ];

    /// Judged from the security types the access point supports. When it
    /// offers several, the simplest wins. Nothing known counts as a password.
    fn of(supports: impl Fn(CWSecurity) -> bool) -> Self {
        let any = |kinds: &[CWSecurity]| kinds.iter().any(|kind| supports(*kind));
        if any(&Self::OPEN) {
            Self::Open
        } else if any(&Self::USER_NAME) && !any(&Self::PASSWORD) {
            Self::UserName
        } else {
            Self::Password
        }
    }
}

/// How often to retry a call that macOS answered with "Resource busy".
const BUSY_RETRIES: u32 = 2;
/// How long to wait before each retry.
const BUSY_WAIT: Duration = Duration::from_millis(1500);

/// Runs a CoreWLAN call, retrying when macOS answers "Resource busy". That
/// happens while macOS is scanning or joining a network by itself, and
/// clears within a second or two; it isn't a reason to give up on a network.
fn retry_if_busy<T>(
    mut call: impl FnMut() -> std::result::Result<T, Retained<NSError>>,
) -> Result<T> {
    let mut retries = 0;
    loop {
        match call() {
            Ok(value) => return Ok(value),
            Err(err)
                if retries < BUSY_RETRIES && is_busy(&err.domain().to_string(), err.code()) =>
            {
                retries += 1;
                thread::sleep(BUSY_WAIT);
            }
            Err(err) => return Err(map_error(&err)),
        }
    }
}

/// `EBUSY` ("Resource busy"), as CoreWLAN reports it.
fn is_busy(domain: &str, code: isize) -> bool {
    domain == "NSPOSIXErrorDomain" && code == 16
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
    fn security_is_judged_from_what_the_access_point_supports() {
        let only = |kinds: &'static [CWSecurity]| move |kind| kinds.contains(&kind);
        assert_eq!(Security::of(only(&[CWSecurity::None])), Security::Open);
        assert_eq!(Security::of(only(&[CWSecurity::OWE])), Security::Open);
        assert_eq!(
            Security::of(only(&[CWSecurity::WPA2Personal])),
            Security::Password
        );
        assert_eq!(
            Security::of(only(&[CWSecurity::WPA3Transition])),
            Security::Password
        );
        assert_eq!(
            Security::of(only(&[CWSecurity::WPA2Enterprise])),
            Security::UserName
        );
        // An access point that takes both a password and a user name.
        assert_eq!(
            Security::of(only(&[
                CWSecurity::WPA2Personal,
                CWSecurity::WPA2Enterprise
            ])),
            Security::Password
        );
        // Unknown: try with the saved password, as before.
        assert_eq!(Security::of(only(&[])), Security::Password);
    }

    #[test]
    fn resource_busy_is_retried_but_other_errors_are_not() {
        assert!(is_busy("NSPOSIXErrorDomain", 16));
        assert!(!is_busy("NSPOSIXErrorDomain", 1));
        assert!(!is_busy("com.apple.coreWLAN.error", 16));
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
