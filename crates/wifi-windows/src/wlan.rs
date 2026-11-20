//! A thin, safe layer over the Native Wifi API (`wlanapi.dll`).
//!
//! Every call opens a [`Session`] (one `WlanOpenHandle`) and closes it when
//! done. A session listens for the scan- and connection-finished
//! notifications, because `WlanScan` and `WlanConnect` return immediately and
//! report the result later.

use std::ffi::c_void;
use std::mem::{offset_of, size_of};
use std::ptr::{self, NonNull};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use thunder_core::{Error, Result};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INVALID_STATE, ERROR_SERVICE_NOT_ACTIVE, ERROR_SUCCESS, HANDLE,
};
use windows::Win32::NetworkManagement::WiFi::{
    DOT11_SSID, L2_NOTIFICATION_DATA, WLAN_API_VERSION_2_0, WLAN_AVAILABLE_NETWORK,
    WLAN_AVAILABLE_NETWORK_HAS_PROFILE, WLAN_AVAILABLE_NETWORK_LIST, WLAN_BSS_ENTRY, WLAN_BSS_LIST,
    WLAN_CONNECTION_ATTRIBUTES, WLAN_CONNECTION_NOTIFICATION_DATA, WLAN_CONNECTION_PARAMETERS,
    WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST, WLAN_NOTIFICATION_SOURCE_ACM,
    WLAN_NOTIFICATION_SOURCE_NONE, WLAN_PROFILE_INFO, WLAN_PROFILE_INFO_LIST, WLAN_RADIO_STATE,
    WLAN_REASON_CODE_MSMSEC_BASE, WLAN_REASON_CODE_MSMSEC_END, WLAN_REASON_CODE_SUCCESS,
    WlanCloseHandle, WlanConnect, WlanEnumInterfaces, WlanFreeMemory, WlanGetAvailableNetworkList,
    WlanGetNetworkBssList, WlanGetProfileList, WlanOpenHandle, WlanQueryInterface,
    WlanRegisterNotification, WlanScan, dot11_BSS_type_any, dot11_BSS_type_infrastructure,
    dot11_radio_state_on, wlan_connection_mode_profile, wlan_interface_state_connected,
    wlan_intf_opcode_current_connection, wlan_intf_opcode_radio_state,
    wlan_notification_acm_connection_attempt_fail, wlan_notification_acm_connection_complete,
    wlan_notification_acm_scan_complete, wlan_notification_acm_scan_fail,
};
use windows::core::{GUID, HRESULT, HSTRING, PCWSTR};

/// A Wi-Fi adapter.
#[derive(Debug, Clone, Copy)]
pub struct Interface {
    /// Identifies the adapter in every other call.
    pub guid: GUID,
    /// Whether it's connected to a network.
    pub connected: bool,
}

/// The current connection on an adapter.
#[derive(Debug, Clone)]
pub struct Connection {
    /// Network name.
    pub ssid: String,
    /// Access point address.
    pub bssid: [u8; 6],
    /// Windows' 0–100 signal quality (used only if the RSSI can't be found).
    pub quality: u32,
}

/// One access point from the BSS list.
#[derive(Debug, Clone)]
pub struct Bss {
    /// Network name (empty for hidden networks).
    pub ssid: String,
    /// Access point address.
    pub bssid: [u8; 6],
    /// Signal strength in dBm.
    pub rssi: i32,
    /// Wi-Fi channel, if the frequency maps to one.
    pub channel: Option<u32>,
}

/// A network from the available-network list.
#[derive(Debug, Clone)]
pub struct Available {
    /// Network name.
    pub ssid: String,
    /// The saved profile Windows would use to join it, if any.
    pub profile: Option<String>,
}

/// What a session heard from Windows.
#[derive(Debug, Clone)]
struct Notification {
    interface: GUID,
    code: u32,
    reason: u32,
    profile: Option<String>,
}

#[derive(Default)]
struct Notifications {
    queue: Mutex<Vec<Notification>>,
    arrived: Condvar,
}

impl Notifications {
    fn queue(&self) -> MutexGuard<'_, Vec<Notification>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Called by wlanapi on its own thread for every ACM notification.
unsafe extern "system" fn on_notification(data: *mut L2_NOTIFICATION_DATA, context: *mut c_void) {
    // SAFETY: wlanapi passes valid data for the duration of the call, and the
    // context is the `Notifications` the session keeps alive until it has
    // unregistered (which waits for running callbacks).
    let (Some(data), Some(notifications)) = (unsafe { data.as_ref() }, unsafe {
        (context as *const Notifications).as_ref()
    }) else {
        return;
    };
    if data.NotificationSource != WLAN_NOTIFICATION_SOURCE_ACM {
        return;
    }

    let code = data.NotificationCode;
    let mut reason = WLAN_REASON_CODE_SUCCESS;
    let mut profile = None;
    let size = data.dwDataSize as usize;
    if code == wlan_notification_acm_scan_fail.0 as u32 && size >= size_of::<u32>() {
        // SAFETY: for scan_fail, pData is a WLAN_REASON_CODE.
        reason = unsafe { *(data.pData as *const u32) };
    } else if (code == wlan_notification_acm_connection_complete.0 as u32
        || code == wlan_notification_acm_connection_attempt_fail.0 as u32)
        && size >= offset_of!(WLAN_CONNECTION_NOTIFICATION_DATA, wlanReasonCode) + size_of::<u32>()
    {
        // SAFETY: for connection notifications, pData is a
        // WLAN_CONNECTION_NOTIFICATION_DATA at least `size` bytes long.
        let connection = unsafe { &*(data.pData as *const WLAN_CONNECTION_NOTIFICATION_DATA) };
        reason = connection.wlanReasonCode;
        profile = Some(wide(&connection.strProfileName));
    }

    notifications.queue().push(Notification {
        interface: data.InterfaceGuid,
        code,
        reason,
        profile,
    });
    notifications.arrived.notify_all();
}

/// Memory returned by wlanapi, freed with `WlanFreeMemory`.
struct WlanMemory<T>(NonNull<T>);

impl<T> WlanMemory<T> {
    fn new(ptr: *mut T) -> Result<Self> {
        NonNull::new(ptr)
            .map(Self)
            .ok_or_else(|| Error::Os("Windows returned no data".into()))
    }

    fn as_ptr(&self) -> *const T {
        self.0.as_ptr()
    }
}

impl<T> Drop for WlanMemory<T> {
    fn drop(&mut self) {
        // SAFETY: the pointer came from wlanapi and is freed once.
        unsafe { WlanFreeMemory(self.0.as_ptr() as *const c_void) };
    }
}

/// One `WlanOpenHandle` session.
pub struct Session {
    handle: HANDLE,
    notifications: *const Notifications,
}

impl Session {
    /// Opens a session with the WLAN service.
    pub fn open() -> Result<Self> {
        let mut version = 0;
        let mut handle = HANDLE::default();
        // SAFETY: out-pointers to locals.
        check(unsafe { WlanOpenHandle(WLAN_API_VERSION_2_0, None, &mut version, &mut handle) })?;

        let notifications = Arc::into_raw(Arc::new(Notifications::default()));
        // SAFETY: the context outlives the registration (see `Drop`).
        let registered = unsafe {
            WlanRegisterNotification(
                handle,
                WLAN_NOTIFICATION_SOURCE_ACM,
                true,
                Some(on_notification),
                Some(notifications as *const c_void),
                None,
                None,
            )
        };
        if registered != ERROR_SUCCESS.0 {
            // SAFETY: undo the open; the context was never handed out.
            unsafe {
                WlanCloseHandle(handle, None);
                drop(Arc::from_raw(notifications));
            }
            return Err(win_error(registered));
        }
        Ok(Self {
            handle,
            notifications,
        })
    }

    fn notifications(&self) -> &Notifications {
        // SAFETY: alive until `drop`.
        unsafe { &*self.notifications }
    }

    /// Every Wi-Fi adapter.
    pub fn interfaces(&self) -> Result<Vec<Interface>> {
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = ptr::null_mut();
        // SAFETY: out-pointer to a local; freed by WlanMemory.
        check(unsafe { WlanEnumInterfaces(self.handle, None, &mut list) })?;
        let list = WlanMemory::new(list)?;
        // SAFETY: wlanapi returns `dwNumberOfItems` entries after the header.
        let items: &[WLAN_INTERFACE_INFO] = unsafe {
            let count = (*list.as_ptr()).dwNumberOfItems as usize;
            std::slice::from_raw_parts(ptr::addr_of!((*list.as_ptr()).InterfaceInfo).cast(), count)
        };
        Ok(items
            .iter()
            .map(|info| Interface {
                guid: info.InterfaceGuid,
                connected: info.isState == wlan_interface_state_connected,
            })
            .collect())
    }

    /// The adapter to use: the connected one, else the first.
    pub fn primary_interface(&self) -> Result<GUID> {
        let interfaces = self.interfaces()?;
        interfaces
            .iter()
            .find(|interface| interface.connected)
            .or(interfaces.first())
            .map(|interface| interface.guid)
            .ok_or_else(|| Error::Os("this computer has no Wi-Fi adapter".into()))
    }

    /// The adapter's current connection, or `None` if it isn't connected.
    pub fn current(&self, interface: &GUID) -> Result<Option<Connection>> {
        let mut size = 0;
        let mut data: *mut c_void = ptr::null_mut();
        // SAFETY: out-pointers to locals; freed by WlanMemory.
        let code = unsafe {
            WlanQueryInterface(
                self.handle,
                interface,
                wlan_intf_opcode_current_connection,
                None,
                &mut size,
                &mut data,
                None,
            )
        };
        if code == ERROR_INVALID_STATE.0 {
            // Not connected.
            return Ok(None);
        }
        check(code)?;
        let data = WlanMemory::new(data as *mut WLAN_CONNECTION_ATTRIBUTES)?;
        // SAFETY: wlanapi returns a WLAN_CONNECTION_ATTRIBUTES for this opcode.
        let attributes = unsafe { &*data.as_ptr() };
        if attributes.isState != wlan_interface_state_connected {
            return Ok(None);
        }
        let association = &attributes.wlanAssociationAttributes;
        Ok(Some(Connection {
            ssid: ssid_text(&association.dot11Ssid),
            bssid: association.dot11Bssid,
            quality: association.wlanSignalQuality,
        }))
    }

    /// Every access point the adapter knows about (from the last scan).
    pub fn bss_list(&self, interface: &GUID) -> Result<Vec<Bss>> {
        let mut list: *mut WLAN_BSS_LIST = ptr::null_mut();
        // SAFETY: out-pointer to a local; freed by WlanMemory.
        check(unsafe {
            WlanGetNetworkBssList(
                self.handle,
                interface,
                None,
                dot11_BSS_type_any,
                false,
                None,
                &mut list,
            )
        })?;
        let list = WlanMemory::new(list)?;
        // SAFETY: `dwNumberOfItems` entries follow the header.
        let entries: &[WLAN_BSS_ENTRY] = unsafe {
            let count = (*list.as_ptr()).dwNumberOfItems as usize;
            std::slice::from_raw_parts(ptr::addr_of!((*list.as_ptr()).wlanBssEntries).cast(), count)
        };
        Ok(entries
            .iter()
            .map(|entry| Bss {
                ssid: ssid_text(&entry.dot11Ssid),
                bssid: entry.dot11Bssid,
                rssi: entry.lRssi,
                channel: channel(entry.ulChCenterFrequency),
            })
            .collect())
    }

    /// Networks in range, with the saved profile for each (if any).
    pub fn available(&self, interface: &GUID) -> Result<Vec<Available>> {
        let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = ptr::null_mut();
        // SAFETY: out-pointer to a local; freed by WlanMemory.
        check(unsafe { WlanGetAvailableNetworkList(self.handle, interface, 0, None, &mut list) })?;
        let list = WlanMemory::new(list)?;
        // SAFETY: `dwNumberOfItems` entries follow the header.
        let networks: &[WLAN_AVAILABLE_NETWORK] = unsafe {
            let count = (*list.as_ptr()).dwNumberOfItems as usize;
            std::slice::from_raw_parts(ptr::addr_of!((*list.as_ptr()).Network).cast(), count)
        };
        Ok(networks
            .iter()
            .map(|network| {
                let profile = wide(&network.strProfileName);
                Available {
                    ssid: ssid_text(&network.dot11Ssid),
                    profile: (network.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0
                        && !profile.is_empty())
                    .then_some(profile),
                }
            })
            .collect())
    }

    /// Names of the profiles Windows has saved for this adapter.
    pub fn profiles(&self, interface: &GUID) -> Result<Vec<String>> {
        let mut list: *mut WLAN_PROFILE_INFO_LIST = ptr::null_mut();
        // SAFETY: out-pointer to a local; freed by WlanMemory.
        check(unsafe { WlanGetProfileList(self.handle, interface, None, &mut list) })?;
        let list = WlanMemory::new(list)?;
        // SAFETY: `dwNumberOfItems` entries follow the header.
        let profiles: &[WLAN_PROFILE_INFO] = unsafe {
            let count = (*list.as_ptr()).dwNumberOfItems as usize;
            std::slice::from_raw_parts(ptr::addr_of!((*list.as_ptr()).ProfileInfo).cast(), count)
        };
        Ok(profiles
            .iter()
            .map(|profile| wide(&profile.strProfileName))
            .collect())
    }

    /// Whether the adapter's radio is on (both the switch and the setting).
    pub fn radio_on(&self, interface: &GUID) -> bool {
        let mut size = 0;
        let mut data: *mut c_void = ptr::null_mut();
        // SAFETY: out-pointers to locals; freed by WlanMemory.
        let code = unsafe {
            WlanQueryInterface(
                self.handle,
                interface,
                wlan_intf_opcode_radio_state,
                None,
                &mut size,
                &mut data,
                None,
            )
        };
        let Ok(data) = check(code).and_then(|()| WlanMemory::new(data as *mut WLAN_RADIO_STATE))
        else {
            // Can't tell: assume on, so the app keeps trying.
            return true;
        };
        // SAFETY: wlanapi returns a WLAN_RADIO_STATE for this opcode.
        let state = unsafe { &*data.as_ptr() };
        let phys = (state.dwNumberOfPhys as usize).min(state.PhyRadioState.len());
        state.PhyRadioState[..phys].iter().any(|phy| {
            phy.dot11SoftwareRadioState == dot11_radio_state_on
                && phy.dot11HardwareRadioState == dot11_radio_state_on
        })
    }

    /// Starts a scan and waits for it to finish (Windows drivers must finish
    /// within 4 seconds). A failed or slow scan isn't an error: the BSS list
    /// still has the results of the last one.
    pub fn scan_and_wait(&self, interface: &GUID, timeout: Duration) -> Result<()> {
        self.notifications().queue().clear();
        // SAFETY: plain call with this session's handle.
        check(unsafe { WlanScan(self.handle, interface, None, None, None) })?;
        let finished = [
            wlan_notification_acm_scan_complete.0 as u32,
            wlan_notification_acm_scan_fail.0 as u32,
        ];
        self.wait(interface, timeout, |n| finished.contains(&n.code));
        Ok(())
    }

    /// Joins a network with a saved profile and waits for the result.
    pub fn connect_and_wait(
        &self,
        interface: &GUID,
        profile: &str,
        timeout: Duration,
    ) -> Result<()> {
        let name = HSTRING::from(profile);
        let parameters = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: PCWSTR(name.as_ptr()),
            pDot11Ssid: ptr::null_mut(),
            pDesiredBssidList: ptr::null_mut(),
            dot11BssType: dot11_BSS_type_infrastructure,
            dwFlags: 0,
        };
        self.notifications().queue().clear();
        // SAFETY: `parameters` and `name` live until WlanConnect returns.
        check(unsafe { WlanConnect(self.handle, interface, &parameters, None) })?;

        let finished = [
            wlan_notification_acm_connection_complete.0 as u32,
            wlan_notification_acm_connection_attempt_fail.0 as u32,
        ];
        let result = self.wait(interface, timeout, |n| {
            finished.contains(&n.code) && n.profile.as_deref().is_none_or(|p| p == profile)
        });
        match result {
            None => Err(Error::Timeout),
            Some(n)
                if n.reason == WLAN_REASON_CODE_SUCCESS
                    && n.code == wlan_notification_acm_connection_complete.0 as u32 =>
            {
                Ok(())
            }
            Some(n) => Err(reason_error(n.reason)),
        }
    }

    /// Waits for a notification about `interface` that matches `wanted`.
    fn wait(
        &self,
        interface: &GUID,
        timeout: Duration,
        wanted: impl Fn(&Notification) -> bool,
    ) -> Option<Notification> {
        let notifications = self.notifications();
        let deadline = Instant::now() + timeout;
        let mut queue = notifications.queue();
        loop {
            if let Some(index) = queue
                .iter()
                .position(|n| n.interface == *interface && wanted(n))
            {
                return Some(queue.remove(index));
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            queue = notifications
                .arrived
                .wait_timeout(queue, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: unregistering and closing both wait for running callbacks,
        // so the context can be freed afterwards.
        unsafe {
            WlanRegisterNotification(
                self.handle,
                WLAN_NOTIFICATION_SOURCE_NONE,
                true,
                None,
                None,
                None,
                None,
            );
            WlanCloseHandle(self.handle, None);
            drop(Arc::from_raw(self.notifications));
        }
    }
}

/// Turns a Win32 error code into `Ok` or the core's error type.
pub fn check(code: u32) -> Result<()> {
    if code == ERROR_SUCCESS.0 {
        Ok(())
    } else {
        Err(win_error(code))
    }
}

/// The core's error for a Win32 error code.
pub fn win_error(code: u32) -> Error {
    match code {
        // Windows 11 24H2+: location is off for desktop apps.
        c if c == ERROR_ACCESS_DENIED.0 => Error::PermissionDenied,
        c if c == ERROR_SERVICE_NOT_ACTIVE.0 => {
            Error::Os("Windows' WLAN AutoConfig service isn't running".into())
        }
        _ => {
            let message = HRESULT::from_win32(code).message();
            Error::Os(format!("{} (error {code})", message.trim()))
        }
    }
}

/// The core's error for a failed connection's WLAN reason code.
///
/// (`WlanReasonCodeToString` would give a nicer message, but the `windows`
/// crate binds its output buffer as read-only, so it isn't used.)
pub fn reason_error(reason: u32) -> Error {
    if is_security_reason(reason) {
        Error::AuthFailed
    } else {
        Error::Os(format!(
            "Windows couldn't connect (WLAN reason code {reason:#x})"
        ))
    }
}

/// Reason codes from the security module (wrong key, failed handshake).
pub fn is_security_reason(reason: u32) -> bool {
    (WLAN_REASON_CODE_MSMSEC_BASE..=WLAN_REASON_CODE_MSMSEC_END).contains(&reason)
}

/// A DOT11_SSID as text.
pub fn ssid_text(ssid: &DOT11_SSID) -> String {
    let len = (ssid.uSSIDLength as usize).min(ssid.ucSSID.len());
    String::from_utf8_lossy(&ssid.ucSSID[..len]).into_owned()
}

/// A NUL-terminated UTF-16 buffer as text.
pub fn wide(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

/// The Wi-Fi channel for a centre frequency in kHz.
pub fn channel(frequency_khz: u32) -> Option<u32> {
    let mhz = frequency_khz / 1000;
    match mhz {
        2412..=2472 => Some((mhz - 2407) / 5),
        2484 => Some(14),
        5160..=5885 => Some((mhz - 5000) / 5),
        5955..=7115 => Some((mhz - 5950) / 5),
        _ => None,
    }
}

/// Windows' 0–100 signal quality as approximate dBm (100 → -50, 0 → -100).
pub fn quality_to_dbm(quality: u32) -> i32 {
    quality.min(100) as i32 / 2 - 100
}

/// A BSSID as `aa:bb:cc:dd:ee:ff`.
pub fn bssid_text(bssid: &[u8; 6]) -> String {
    bssid
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssids_and_wide_strings_become_text() {
        let mut raw = DOT11_SSID {
            uSSIDLength: 7,
            ucSSID: [0; 32],
        };
        raw.ucSSID[..7].copy_from_slice(b"Dorm-5G");
        assert_eq!(ssid_text(&raw), "Dorm-5G");
        raw.uSSIDLength = 99;
        assert_eq!(ssid_text(&raw).trim_end_matches('\0'), "Dorm-5G");

        let mut buffer = [0u16; 16];
        for (slot, c) in buffer.iter_mut().zip("Kos Melati".encode_utf16()) {
            *slot = c;
        }
        assert_eq!(wide(&buffer), "Kos Melati");
    }

    #[test]
    fn frequencies_map_to_channels() {
        assert_eq!(channel(2_412_000), Some(1));
        assert_eq!(channel(2_437_000), Some(6));
        assert_eq!(channel(2_484_000), Some(14));
        assert_eq!(channel(5_180_000), Some(36));
        assert_eq!(channel(5_745_000), Some(149));
        assert_eq!(channel(5_975_000), Some(5));
        assert_eq!(channel(0), None);
    }

    #[test]
    fn quality_becomes_dbm() {
        assert_eq!(quality_to_dbm(100), -50);
        assert_eq!(quality_to_dbm(50), -75);
        assert_eq!(quality_to_dbm(0), -100);
        assert_eq!(quality_to_dbm(250), -50);
    }

    #[test]
    fn bssid_formatting() {
        assert_eq!(
            bssid_text(&[0xd8, 0x85, 0x5e, 0x6a, 0x78, 0x68]),
            "d8:85:5e:6a:78:68"
        );
    }

    #[test]
    fn errors_map_to_the_core_error() {
        assert_eq!(win_error(5), Error::PermissionDenied);
        assert!(matches!(win_error(1062), Error::Os(m) if m.contains("WLAN AutoConfig")));
        assert!(is_security_reason(WLAN_REASON_CODE_MSMSEC_BASE + 1));
        assert!(!is_security_reason(WLAN_REASON_CODE_SUCCESS));
        assert_eq!(
            reason_error(WLAN_REASON_CODE_MSMSEC_BASE + 1),
            Error::AuthFailed
        );
    }
}
