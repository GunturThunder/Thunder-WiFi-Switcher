//! Router address for the Wi-Fi adapter, from the IPv4 routing table.

use std::net::{IpAddr, Ipv4Addr};
use std::ptr;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceGuidToLuid, FreeMibTable, GetIpForwardTable2, MIB_IPFORWARD_ROW2,
    MIB_IPFORWARD_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows::Win32::Networking::WinSock::AF_INET;
use windows::core::GUID;

/// The next hop of the Wi-Fi adapter's default route (lowest metric), or
/// `None` if it has none (disconnected, or no router).
pub fn router_for(interface: &GUID) -> Option<IpAddr> {
    let mut luid = NET_LUID_LH::default();
    // SAFETY: out-pointer to a local.
    if unsafe { ConvertInterfaceGuidToLuid(interface, &mut luid) } != ERROR_SUCCESS {
        return None;
    }
    // SAFETY: both union views are a plain u64.
    let luid = unsafe { luid.Value };

    let mut table: *mut MIB_IPFORWARD_TABLE2 = ptr::null_mut();
    // SAFETY: out-pointer to a local; freed below.
    if unsafe { GetIpForwardTable2(AF_INET, &mut table) } != ERROR_SUCCESS || table.is_null() {
        return None;
    }
    // SAFETY: `NumEntries` rows follow the header.
    let rows: &[MIB_IPFORWARD_ROW2] = unsafe {
        let count = (*table).NumEntries as usize;
        std::slice::from_raw_parts(ptr::addr_of!((*table).Table).cast(), count)
    };

    let router = rows
        .iter()
        .filter(|row| {
            // SAFETY: plain u64 union view.
            row.DestinationPrefix.PrefixLength == 0 && unsafe { row.InterfaceLuid.Value } == luid
        })
        .min_by_key(|row| row.Metric)
        .and_then(|row| {
            // SAFETY: an AF_INET table holds IPv4 next hops.
            let raw = unsafe { row.NextHop.Ipv4.sin_addr.S_un.S_addr };
            // S_addr is in network byte order.
            let address = Ipv4Addr::from(raw.to_ne_bytes());
            (!address.is_unspecified()).then_some(IpAddr::V4(address))
        });

    // SAFETY: the table came from GetIpForwardTable2.
    unsafe { FreeMibTable(table as *const _) };
    router
}
