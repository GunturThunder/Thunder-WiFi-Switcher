//! Ping without admin rights: macOS allows ICMP echo on a datagram
//! (`SOCK_DGRAM`) socket for any user.

use std::io;
use std::mem::MaybeUninit;
use std::net::{IpAddr, SocketAddrV4};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use thunder_core::{Error, Result};

const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;
/// Header (8 bytes) plus an 8-byte token that identifies our probe.
const PACKET_LEN: usize = 16;

static SEQUENCE: AtomicU16 = AtomicU16::new(1);

/// Sends one echo request to `addr` and waits for the reply.
pub fn ping(addr: IpAddr, timeout: Duration) -> Result<Duration> {
    let IpAddr::V4(target) = addr else {
        // Falls back to the TCP check.
        return Err(Error::Os("ping over IPv6 isn't supported yet".into()));
    };
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).map_err(os)?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let token = token(sequence);
    let packet = echo_request(std::process::id() as u16, sequence, &token);

    let started = Instant::now();
    socket
        .send_to(&packet, &SockAddr::from(SocketAddrV4::new(target, 0)))
        .map_err(os)?;

    let mut buffer = [MaybeUninit::<u8>::uninit(); 1500];
    loop {
        let left = timeout
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
            .ok_or(Error::Timeout)?;
        socket.set_read_timeout(Some(left)).map_err(os)?;
        let (len, from) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(Error::Timeout);
            }
            Err(err) => return Err(os(err)),
        };
        // SAFETY: recv_from initialised the first `len` bytes.
        let data = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), len) };
        let from_target = from
            .as_socket_ipv4()
            .is_some_and(|from| *from.ip() == target);
        if from_target && is_our_reply(data, sequence, &token) {
            return Ok(started.elapsed());
        }
        // Someone else's ICMP (the socket can see other replies): keep waiting.
    }
}

fn os(err: io::Error) -> Error {
    Error::Os(err.to_string())
}

/// A value no other probe will carry, so replies can't be confused.
fn token(sequence: u16) -> [u8; 8] {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.subsec_nanos());
    let mut token = [0; 8];
    token[..2].copy_from_slice(&sequence.to_be_bytes());
    token[2..6].copy_from_slice(&nanos.to_be_bytes());
    token[6..].copy_from_slice(b"TW");
    token
}

fn echo_request(id: u16, sequence: u16, token: &[u8; 8]) -> [u8; PACKET_LEN] {
    let mut packet = [0; PACKET_LEN];
    packet[0] = ECHO_REQUEST;
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());
    packet[8..].copy_from_slice(token);
    let sum = checksum(&packet);
    packet[2..4].copy_from_slice(&sum.to_be_bytes());
    packet
}

/// The Internet checksum (RFC 1071).
fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = data
        .chunks(2)
        .map(|pair| u32::from(u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)])))
        .sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Whether `data` is the echo reply to our probe. macOS hands datagram ICMP
/// sockets the IPv4 header too; ICMP types never start with 0x4_, so a first
/// byte of 0x45-0x4f means "skip the header".
fn is_our_reply(data: &[u8], sequence: u16, token: &[u8; 8]) -> bool {
    let icmp = match data.first() {
        Some(first) if first >> 4 == 4 => data.get(usize::from(first & 0x0f) * 4..).unwrap_or(&[]),
        _ => data,
    };
    icmp.len() >= PACKET_LEN
        && icmp[0] == ECHO_REPLY
        && icmp[6..8] == sequence.to_be_bytes()
        && icmp[8..PACKET_LEN] == token[..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout_and_checksum() {
        let packet = echo_request(0x1234, 7, b"ABCDEFGH");
        assert_eq!(packet[0], ECHO_REQUEST);
        assert_eq!(&packet[4..8], &[0x12, 0x34, 0, 7]);
        assert_eq!(&packet[8..], b"ABCDEFGH");
        // A packet with a correct checksum sums to zero.
        assert_eq!(checksum(&packet), 0);
    }

    #[test]
    fn recognises_our_reply_with_or_without_an_ip_header() {
        let token = *b"ABCDEFGH";
        let mut reply = echo_request(1, 9, &token);
        reply[0] = ECHO_REPLY;
        assert!(is_our_reply(&reply, 9, &token));
        assert!(!is_our_reply(&reply, 10, &token));
        assert!(!is_our_reply(&reply, 9, b"XXXXXXXX"));

        let mut with_header = vec![0x45; 1];
        with_header.extend([0; 19]);
        with_header.extend(reply);
        assert!(is_our_reply(&with_header, 9, &token));

        let mut request = echo_request(1, 9, &token);
        request[0] = ECHO_REQUEST;
        assert!(!is_our_reply(&request, 9, &token));
    }

    #[test]
    fn pings_this_computer() {
        // Loopback answers on any Mac, including CI, without a network.
        let rtt = ping("127.0.0.1".parse().unwrap(), Duration::from_secs(1)).unwrap();
        assert!(rtt < Duration::from_secs(1));
    }
}
