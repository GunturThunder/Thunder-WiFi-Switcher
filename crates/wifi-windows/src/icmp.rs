//! Ping without admin rights, with Windows' ICMP helper (`IcmpSendEcho`).

use std::ffi::c_void;
use std::mem::size_of;
use std::net::IpAddr;
use std::time::Duration;

use thunder_core::{Error, Result};
use windows::Win32::NetworkManagement::IpHelper::{
    ICMP_ECHO_REPLY, IP_SUCCESS, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
};

/// Payload sent with each echo request.
const PAYLOAD: &[u8; 16] = b"ThunderWiFiProbe";

/// Sends one echo request to `addr` and waits for the reply.
pub fn ping(addr: IpAddr, timeout: Duration) -> Result<Duration> {
    let IpAddr::V4(target) = addr else {
        // Falls back to the TCP check.
        return Err(Error::Os("ping over IPv6 isn't supported yet".into()));
    };

    // Room for one reply, the echoed payload, an ICMP error and some slack,
    // in u64s so the reply struct is properly aligned.
    let bytes = size_of::<ICMP_ECHO_REPLY>() + PAYLOAD.len() + 8 + 16;
    let mut reply = vec![0u64; bytes.div_ceil(8)];
    let timeout_ms = u32::try_from(timeout.as_millis())
        .unwrap_or(u32::MAX)
        .max(1);

    // SAFETY: the handle is closed below; the buffers outlive the call.
    let replies = unsafe {
        let handle = IcmpCreateFile().map_err(|err| Error::Os(err.message()))?;
        let replies = IcmpSendEcho(
            handle,
            // The address in network byte order, as laid out in memory.
            u32::from_ne_bytes(target.octets()),
            PAYLOAD.as_ptr().cast::<c_void>(),
            PAYLOAD.len() as u16,
            None,
            reply.as_mut_ptr().cast::<c_void>(),
            (reply.len() * 8) as u32,
            timeout_ms,
        );
        let _ = IcmpCloseHandle(handle);
        replies
    };
    if replies == 0 {
        return Err(Error::Timeout);
    }
    // SAFETY: with at least one reply, the buffer starts with an ICMP_ECHO_REPLY.
    let echo = unsafe { &*reply.as_ptr().cast::<ICMP_ECHO_REPLY>() };
    if echo.Status != IP_SUCCESS {
        // Unreachable, TTL expired, …: no answer from the target itself.
        return Err(Error::Timeout);
    }
    Ok(Duration::from_millis(echo.RoundTripTime.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pings_this_computer() {
        // Loopback answers on any Windows machine, including CI.
        let rtt = ping("127.0.0.1".parse().unwrap(), Duration::from_secs(1)).unwrap();
        assert!(rtt < Duration::from_secs(1));
    }

    #[test]
    fn ipv6_falls_back_to_tcp() {
        assert!(matches!(
            ping("::1".parse().unwrap(), Duration::from_secs(1)),
            Err(Error::Os(_))
        ));
    }
}
