//! Router address, from the System Configuration dynamic store (the same
//! source `route -n get default` and System Settings use).

use std::net::IpAddr;

use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFType};
use objc2_system_configuration::SCDynamicStore;

/// The router for `interface` (e.g. `en0`), if it carries the Mac's default
/// IPv4 route. `None` when disconnected, or when another connection (such as
/// Ethernet) is the default: the app only judges the Wi-Fi router.
pub fn router_for(interface: &str) -> Option<IpAddr> {
    let key = CFString::from_static_str("State:/Network/Global/IPv4");
    let value = SCDynamicStore::value(None, &key)?;
    let dict = value.downcast::<CFDictionary>().ok()?;
    // SAFETY: the dynamic store's IPv4 entry is a dictionary with string keys.
    let dict: CFRetained<CFDictionary<CFString, CFType>> =
        unsafe { CFRetained::cast_unchecked(dict) };

    let text = |name: &'static str| -> Option<String> {
        let value = dict.get(&CFString::from_static_str(name))?;
        Some(value.downcast::<CFString>().ok()?.to_string())
    };

    if text("PrimaryInterface")? != interface {
        return None;
    }
    text("Router")?.parse().ok()
}
