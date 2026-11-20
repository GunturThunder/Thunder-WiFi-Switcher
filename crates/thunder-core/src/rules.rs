//! The switching rules from the PRD, as constants and pure functions.
//!
//! Only the weak-signal level is user-adjustable (it lives in
//! [`Config`]); everything here is fixed so behaviour stays predictable.

use std::time::Duration;

use crate::config::{Config, Network};
use crate::engine::{Reason, Sample};
use crate::quality::Internet;
use crate::wifi::ScanResult;

/// How often the app checks signal (and, from Phase 4, ping).
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
/// A problem must show up in this many checks in a row (15 s) before the app
/// looks for another network. A dropped connection doesn't wait.
pub const BAD_SAMPLES_BEFORE_SWITCH: u32 = 3;
/// Another network must be at least this much stronger (dB) to be worth it.
pub const SIGNAL_MARGIN_DB: i32 = 8;
/// Candidates within this many dB of the strongest count as equal, and the
/// one higher in the user's list wins.
pub const PRIORITY_TIE_DB: i32 = 3;
/// After a switch, give the new network this long before judging it.
pub const COOLDOWN_AFTER_SWITCH: Duration = Duration::from_secs(30);
/// After waking from sleep, give the OS this long to rejoin a network itself.
pub const WAKE_GRACE: Duration = Duration::from_secs(15);
/// First wait when nothing better is in range; doubles each time.
pub const BACKOFF_START: Duration = Duration::from_secs(30);
/// Longest wait between retries.
pub const BACKOFF_MAX: Duration = Duration::from_secs(300);
/// A network that failed to connect is skipped for this long.
pub const AVOID_AFTER_FAILED_CONNECT: Duration = Duration::from_secs(10 * 60);
/// A network the app left because of bad internet is skipped for this long.
pub const AVOID_AFTER_BAD_INTERNET: Duration = Duration::from_secs(10 * 60);
/// A scan or connect that hasn't answered after this long counts as failed.
pub const STUCK_TIMEOUT: Duration = Duration::from_secs(60);

/// What one sample means for the current network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assessment {
    /// Connected to one of the user's networks, and it's fine.
    Fine,
    /// Something is wrong that another network might fix.
    Problem(Reason),
    /// Not the app's business: Wi-Fi is off, or the user is on a network
    /// they haven't added (a café, a phone hotspot), or has turned off.
    HandsOff,
}

/// Judges one sample against the user's settings.
pub fn assess(sample: &Sample, config: &Config) -> Assessment {
    match sample {
        Sample::WifiOff => Assessment::HandsOff,
        Sample::Disconnected if config.enabled_networks().next().is_none() => Assessment::HandsOff,
        Sample::Disconnected => Assessment::Problem(Reason::Disconnected),
        Sample::Connected(link, _) if !config.is_enabled(&link.ssid) => Assessment::HandsOff,
        // A weak signal explains everything else, so it's checked first.
        Sample::Connected(link, _) if link.rssi < config.weak_signal_dbm => {
            Assessment::Problem(Reason::WeakSignal { rssi: link.rssi })
        }
        Sample::Connected(_, Internet::BadLink { ping_ms, loss_pct }) => {
            Assessment::Problem(Reason::BadLink {
                ping_ms: *ping_ms,
                loss_pct: *loss_pct,
            })
        }
        Sample::Connected(_, Internet::Bad { ping_ms, loss_pct }) => {
            Assessment::Problem(Reason::BadInternet {
                ping_ms: *ping_ms,
                loss_pct: *loss_pct,
            })
        }
        Sample::Connected(..) => Assessment::Fine,
    }
}

/// What a candidate's signal must be to be worth switching to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// Weak signal: at least [`SIGNAL_MARGIN_DB`] stronger than this (dBm).
    StrongerThan(i32),
    /// Bad internet: any signal at or above this (the weak-signal level).
    AtLeast(i32),
    /// Disconnected: anything in range.
    Any,
}

impl Requirement {
    fn allows(self, rssi: i32) -> bool {
        match self {
            Self::StrongerThan(current) => rssi >= current + SIGNAL_MARGIN_DB,
            Self::AtLeast(minimum) => rssi >= minimum,
            Self::Any => true,
        }
    }
}

/// Picks the network to switch to from a scan, or `None` if nothing will do.
///
/// - Only enabled networks from the user's list, and not `current` or one in
///   `is_avoided`.
/// - Each network is judged by its strongest access point in the scan, which
///   must meet `requirement`.
/// - Candidates within [`PRIORITY_TIE_DB`] of the strongest are a tie, and the
///   one higher in `networks` wins.
pub fn choose_network(
    scan: &[ScanResult],
    networks: &[Network],
    current: Option<&str>,
    requirement: Requirement,
    is_avoided: impl Fn(&str) -> bool,
) -> Option<String> {
    let candidates: Vec<(usize, &str, i32)> = networks
        .iter()
        .enumerate()
        .filter(|(_, network)| network.enabled && !network.ssid.is_empty())
        .filter(|(_, network)| current != Some(network.ssid.as_str()))
        .filter(|(_, network)| !is_avoided(&network.ssid))
        .filter_map(|(priority, network)| {
            let rssi = scan
                .iter()
                .filter(|result| result.ssid == network.ssid)
                .map(|result| result.rssi)
                .max()?;
            Some((priority, network.ssid.as_str(), rssi))
        })
        .filter(|&(_, _, rssi)| requirement.allows(rssi))
        .collect();

    let strongest = candidates.iter().map(|&(_, _, rssi)| rssi).max()?;
    candidates
        .iter()
        .filter(|&&(_, _, rssi)| rssi >= strongest - PRIORITY_TIE_DB)
        .min_by_key(|&&(priority, _, _)| priority)
        .map(|&(_, ssid, _)| ssid.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wifi::Link;

    fn networks(ssids: &[&str]) -> Vec<Network> {
        ssids.iter().map(|ssid| Network::new(*ssid)).collect()
    }

    fn seen(ssid: &str, rssi: i32) -> ScanResult {
        ScanResult {
            ssid: ssid.into(),
            bssid: None,
            rssi,
            channel: None,
        }
    }

    fn link(ssid: &str, rssi: i32) -> Link {
        Link {
            ssid: ssid.into(),
            bssid: None,
            rssi,
            channel: None,
        }
    }

    fn connected(ssid: &str, rssi: i32, internet: Internet) -> Sample {
        Sample::Connected(link(ssid, rssi), internet)
    }

    fn config(ssids: &[&str]) -> Config {
        Config {
            networks: networks(ssids),
            ..Config::default()
        }
    }

    fn never(_: &str) -> bool {
        false
    }

    #[test]
    fn signal_below_the_threshold_is_a_problem() {
        let config = config(&["A"]);
        // "Below -75 dBm": -75 itself is fine.
        assert_eq!(
            assess(&connected("A", -75, Internet::Good), &config),
            Assessment::Fine
        );
        assert_eq!(
            assess(&connected("A", -76, Internet::Good), &config),
            Assessment::Problem(Reason::WeakSignal { rssi: -76 })
        );
    }

    #[test]
    fn threshold_follows_the_setting() {
        let config = Config {
            weak_signal_dbm: -68,
            ..config(&["A"])
        };
        assert_eq!(
            assess(&connected("A", -70, Internet::Good), &config),
            Assessment::Problem(Reason::WeakSignal { rssi: -70 })
        );
    }

    #[test]
    fn disconnected_is_a_problem_only_with_networks_to_join() {
        assert_eq!(
            assess(&Sample::Disconnected, &config(&["A"])),
            Assessment::Problem(Reason::Disconnected)
        );
        assert_eq!(
            assess(&Sample::Disconnected, &config(&[])),
            Assessment::HandsOff
        );
    }

    #[test]
    fn bad_internet_and_bad_links_are_problems_unless_the_signal_is_weak() {
        let config = config(&["A"]);
        let down = Internet::Bad {
            ping_ms: None,
            loss_pct: 100,
        };
        let router = Internet::BadLink {
            ping_ms: Some(3),
            loss_pct: 50,
        };
        assert_eq!(
            assess(&connected("A", -50, down), &config),
            Assessment::Problem(Reason::BadInternet {
                ping_ms: None,
                loss_pct: 100
            })
        );
        assert_eq!(
            assess(&connected("A", -50, router), &config),
            Assessment::Problem(Reason::BadLink {
                ping_ms: Some(3),
                loss_pct: 50
            })
        );
        assert_eq!(
            assess(&connected("A", -80, down), &config),
            Assessment::Problem(Reason::WeakSignal { rssi: -80 })
        );
        assert_eq!(
            assess(&connected("A", -50, Internet::Unknown), &config),
            Assessment::Fine
        );
    }

    #[test]
    fn leaving_bad_internet_takes_any_network_that_is_not_weak() {
        let nets = networks(&["A", "B", "C"]);
        let scan = [seen("A", -45), seen("B", -74), seen("C", -76)];
        assert_eq!(
            choose_network(&scan, &nets, Some("A"), Requirement::AtLeast(-75), never),
            Some("B".into())
        );
    }

    #[test]
    fn networks_the_user_has_not_added_are_left_alone() {
        let config = config(&["A"]);
        assert_eq!(
            assess(&connected("Cafe", -90, Internet::Good), &config),
            Assessment::HandsOff
        );
        assert_eq!(assess(&Sample::WifiOff, &config), Assessment::HandsOff);
    }

    #[test]
    fn needs_at_least_8_db_more_than_the_current_network() {
        let nets = networks(&["A", "B"]);
        let weaker_than = Requirement::StrongerThan(-80);
        let seven_better = [seen("B", -73)];
        let eight_better = [seen("B", -72)];
        assert_eq!(
            choose_network(&seven_better, &nets, Some("A"), weaker_than, never),
            None
        );
        assert_eq!(
            choose_network(&eight_better, &nets, Some("A"), weaker_than, never),
            Some("B".into())
        );
    }

    #[test]
    fn when_disconnected_any_saved_network_will_do() {
        let nets = networks(&["A", "B"]);
        let scan = [seen("A", -85), seen("B", -88)];
        assert_eq!(
            choose_network(&scan, &nets, None, Requirement::Any, never),
            Some("A".into())
        );
    }

    #[test]
    fn within_3_db_the_higher_priority_network_wins() {
        let nets = networks(&["Dorm-5G", "Dorm-2.4"]);
        let scan = [seen("Dorm-5G", -66), seen("Dorm-2.4", -63)];
        assert_eq!(
            choose_network(&scan, &nets, None, Requirement::Any, never),
            Some("Dorm-5G".into())
        );
    }

    #[test]
    fn more_than_3_db_apart_the_stronger_network_wins() {
        let nets = networks(&["Dorm-5G", "Dorm-2.4"]);
        let scan = [seen("Dorm-5G", -67), seen("Dorm-2.4", -63)];
        assert_eq!(
            choose_network(&scan, &nets, None, Requirement::Any, never),
            Some("Dorm-2.4".into())
        );
    }

    #[test]
    fn uses_the_strongest_access_point_of_each_network() {
        let nets = networks(&["A", "B"]);
        let scan = [seen("B", -85), seen("B", -60), seen("A", -70)];
        assert_eq!(
            choose_network(&scan, &nets, None, Requirement::Any, never),
            Some("B".into())
        );
    }

    #[test]
    fn skips_the_current_disabled_avoided_and_unknown_networks() {
        let mut nets = networks(&["Current", "Off", "Avoided", "Ok"]);
        nets[1].enabled = false;
        let scan = [
            seen("Current", -40),
            seen("Off", -40),
            seen("Avoided", -40),
            seen("Stranger", -40),
            seen("Ok", -70),
        ];
        assert_eq!(
            choose_network(
                &scan,
                &nets,
                Some("Current"),
                Requirement::StrongerThan(-85),
                |ssid| ssid == "Avoided"
            ),
            Some("Ok".into())
        );
    }

    #[test]
    fn nothing_visible_means_no_choice() {
        assert_eq!(
            choose_network(&[], &networks(&["A"]), None, Requirement::Any, never),
            None
        );
    }
}
