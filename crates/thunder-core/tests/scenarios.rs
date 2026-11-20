//! Real-life scenarios from the PRD, run through the real engine and driver
//! against the fake backend, one 5-second check at a time.

use std::sync::Arc;
use std::time::Duration;

use thunder_core::fake::{Call, FakeBackend};
use thunder_core::rules::SAMPLE_INTERVAL;
use thunder_core::{Config, Driver, Error, Event, Network, Notice, Reason};

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn mins(n: u64) -> Duration {
    Duration::from_secs(n * 60)
}

/// A laptop in a building: the fake Wi-Fi, the driver, and a clock.
struct Sim {
    wifi: Arc<FakeBackend>,
    driver: Driver<Arc<FakeBackend>>,
    now: Duration,
    notices: Vec<(Duration, Notice)>,
    scan_times: Vec<Duration>,
}

impl Sim {
    /// The user's networks, in priority order.
    fn new(networks: &[&str]) -> Self {
        Self::with_config(Config {
            networks: networks.iter().map(|ssid| Network::new(*ssid)).collect(),
            ..Config::default()
        })
    }

    fn with_config(config: Config) -> Self {
        let wifi = Arc::new(FakeBackend::new());
        Self {
            driver: Driver::new(config, Arc::clone(&wifi)),
            wifi,
            now: Duration::ZERO,
            notices: Vec::new(),
            scan_times: Vec::new(),
        }
    }

    /// One check, then the clock moves on 5 seconds.
    fn tick(&mut self) {
        let scans_before = self.scans();
        let notices = self
            .driver
            .tick(self.now)
            .expect("the fake can always be read");
        if self.scans() > scans_before {
            self.scan_times.push(self.now);
        }
        self.notices
            .extend(notices.into_iter().map(|notice| (self.now, notice)));
        self.now += SAMPLE_INTERVAL;
    }

    fn run(&mut self, duration: Duration) {
        let end = self.now + duration;
        while self.now < end {
            self.tick();
        }
    }

    fn send(&mut self, event: Event) {
        let notices = self.driver.handle(self.now, event);
        self.notices
            .extend(notices.into_iter().map(|notice| (self.now, notice)));
    }

    /// (time, network) for every successful switch.
    fn switches(&self) -> Vec<(Duration, String)> {
        self.notices
            .iter()
            .filter_map(|(at, notice)| match notice {
                Notice::Switched { to, .. } => Some((*at, to.clone())),
                Notice::ConnectFailed { .. } => None,
            })
            .collect()
    }

    fn scans(&self) -> usize {
        self.wifi
            .calls()
            .iter()
            .filter(|call| **call == Call::Scan)
            .count()
    }

    fn connect_attempts(&self) -> Vec<String> {
        self.wifi
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Connect(ssid) => Some(ssid),
                Call::Scan => None,
            })
            .collect()
    }

    fn connected(&self) -> Option<String> {
        self.wifi.connected()
    }
}

#[test]
fn walking_away_switches_to_the_stronger_network_within_30_seconds() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.set_network("Dorm-B", -72);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));
    assert!(sim.switches().is_empty());

    // Walk down the corridor: Dorm-A fades, Dorm-B gets closer.
    let walked_at = sim.now;
    sim.wifi.set_network("Dorm-A", -81);
    sim.wifi.set_network("Dorm-B", -63);
    sim.run(mins(10));

    let switches = sim.switches();
    assert_eq!(switches.len(), 1, "{switches:?}");
    let (at, to) = &switches[0];
    assert_eq!(to, "Dorm-B");
    assert!(*at - walked_at <= secs(30), "took {:?}", *at - walked_at);
    assert_eq!(
        sim.notices[0].1,
        Notice::Switched {
            from: Some("Dorm-A".into()),
            to: "Dorm-B".into(),
            reason: Reason::WeakSignal { rssi: -81 },
        }
    );
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));
}

#[test]
fn a_short_dip_does_nothing() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.set_network("Dorm-B", -55);
    sim.wifi.join("Dorm-A");

    // Someone closes a door: weak for 10 seconds (2 checks), then fine.
    sim.wifi.set_network("Dorm-A", -82);
    sim.run(secs(10));
    sim.wifi.set_network("Dorm-A", -60);
    sim.run(mins(5));

    assert_eq!(sim.scans(), 0);
    assert!(sim.switches().is_empty());
}

#[test]
fn flapping_signal_never_triggers_a_switch() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -74);
    sim.wifi.set_network("Dorm-B", -62);
    sim.wifi.join("Dorm-A");

    // Dorm-A hovers right at the threshold, weak every other check.
    for check in 0..720 {
        let rssi = if check % 2 == 0 { -74 } else { -77 };
        sim.wifi.set_network("Dorm-A", rssi);
        sim.tick();
    }

    assert_eq!(sim.scans(), 0);
    assert!(sim.switches().is_empty());
}

#[test]
fn two_weak_networks_do_not_ping_pong() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -83);
    sim.wifi.set_network("Dorm-B", -77);
    sim.wifi.join("Dorm-A");

    // An hour in a spot where both networks are weak and wobble. Dorm-B is
    // always 6–7 dB better: close, but not the 8 dB needed. (The pattern
    // repeats every 5 checks, so retries land on different points of it.)
    let a = [-83, -84, -83, -82, -84];
    let b = [-77, -78, -76, -76, -78];
    for check in 0..720 {
        sim.wifi.set_network("Dorm-A", a[check % 5]);
        sim.wifi.set_network("Dorm-B", b[check % 5]);
        sim.tick();
    }

    assert!(sim.switches().is_empty());
    assert!(sim.connect_attempts().is_empty());
    // Backing off keeps scanning rare: 14 scans in the hour, not 240.
    assert!(sim.scans() <= 15, "{} scans", sim.scans());
}

#[test]
fn two_equal_networks_prefer_the_one_higher_in_the_list() {
    let mut sim = Sim::new(&["Dorm-5G", "Dorm-2.4"]);
    sim.wifi.set_network("Dorm-5G", -67);
    sim.wifi.set_network("Dorm-2.4", -65);

    // Not connected yet: join the best network, preferring the list order.
    sim.tick();
    assert_eq!(sim.connected().as_deref(), Some("Dorm-5G"));

    // An hour of normal wobbling stays on it.
    for check in 0..720 {
        sim.wifi.set_network("Dorm-2.4", -62 - check % 5);
        sim.tick();
    }
    assert_eq!(sim.switches().len(), 1);
    assert_eq!(sim.connected().as_deref(), Some("Dorm-5G"));
}

#[test]
fn a_dropped_connection_reconnects_straight_away() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.set_network("Dorm-B", -70);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));

    let dropped_at = sim.now;
    sim.wifi.remove_network("Dorm-A");
    sim.tick();

    assert_eq!(
        sim.notices,
        [(
            dropped_at,
            Notice::Switched {
                from: None,
                to: "Dorm-B".into(),
                reason: Reason::Disconnected,
            }
        )]
    );
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));
}

#[test]
fn nothing_better_in_range_backs_off_from_30_seconds_to_5_minutes() {
    let mut sim = Sim::new(&["Dorm-A"]);
    sim.wifi.set_network("Dorm-A", -80);
    sim.wifi.join("Dorm-A");
    sim.run(mins(20));

    let gaps: Vec<u64> = sim
        .scan_times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).as_secs())
        .collect();
    assert_eq!(
        sim.scan_times[0],
        secs(10),
        "first scan after 3 weak checks"
    );
    assert_eq!(gaps[..6], [30, 60, 120, 240, 300, 300]);
    assert!(sim.connect_attempts().is_empty());
}

#[test]
fn backoff_starts_short_again_after_the_signal_recovers() {
    let mut sim = Sim::new(&["Dorm-A"]);
    sim.wifi.set_network("Dorm-A", -80);
    sim.wifi.join("Dorm-A");
    sim.run(secs(105)); // scans at 10, 40, 100

    sim.wifi.set_network("Dorm-A", -60);
    sim.tick(); // one good check
    sim.wifi.set_network("Dorm-A", -80);
    sim.run(mins(2));

    let gaps: Vec<u64> = sim
        .scan_times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).as_secs())
        .collect();
    // 30, 60, then a fresh problem (3 weak checks), then 30 again.
    assert_eq!(gaps[..4], [30, 60, 20, 30]);
}

#[test]
fn a_network_that_fails_to_connect_is_skipped_for_the_next_best() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B", "Dorm-C"]);
    sim.wifi.set_network("Dorm-A", -82);
    sim.wifi.set_network("Dorm-B", -60);
    sim.wifi.set_network("Dorm-C", -68);
    sim.wifi.fail_connect("Dorm-B", Error::AuthFailed);
    sim.wifi.join("Dorm-A");
    sim.run(mins(3));

    assert_eq!(sim.connect_attempts(), ["Dorm-B", "Dorm-C"]);
    assert_eq!(
        sim.notices
            .iter()
            .map(|(_, n)| n.clone())
            .collect::<Vec<_>>(),
        [
            Notice::ConnectFailed {
                ssid: "Dorm-B".into(),
                error: Error::AuthFailed,
            },
            Notice::Switched {
                from: Some("Dorm-A".into()),
                to: "Dorm-C".into(),
                reason: Reason::WeakSignal { rssi: -82 },
            },
        ]
    );
    assert_eq!(sim.connected().as_deref(), Some("Dorm-C"));
}

#[test]
fn does_not_switch_back_while_the_current_network_is_fine() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -82);
    sim.wifi.set_network("Dorm-B", -64);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));

    // Walk back to your room: Dorm-A is now much stronger, but Dorm-B is fine.
    sim.wifi.set_network("Dorm-A", -50);
    sim.wifi.set_network("Dorm-B", -70);
    sim.run(mins(30));
    assert_eq!(sim.switches().len(), 1);
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));
}

#[test]
fn paused_auto_switch_does_nothing() {
    let mut sim = Sim::with_config(Config {
        auto_switch: false,
        networks: vec![Network::new("Dorm-A"), Network::new("Dorm-B")],
        ..Config::default()
    });
    sim.wifi.set_network("Dorm-A", -85);
    sim.wifi.set_network("Dorm-B", -55);
    sim.wifi.join("Dorm-A");
    sim.run(mins(5));
    sim.wifi.disconnect();
    sim.run(mins(5));

    assert!(sim.wifi.calls().is_empty());
}

#[test]
fn after_waking_up_the_os_gets_time_to_rejoin_by_itself() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));

    sim.send(Event::Sleep);
    // Carried to the library while asleep: different networks in range.
    sim.wifi.remove_network("Dorm-A");
    sim.wifi.set_network("Dorm-B", -65);
    sim.now += mins(60);
    sim.send(Event::Wake);

    // The OS rejoins Dorm-B after 10 seconds; the app stays out of the way.
    sim.run(secs(10));
    sim.wifi.join("Dorm-B");
    sim.run(mins(5));
    assert!(sim.wifi.calls().is_empty());
}

#[test]
fn after_waking_up_the_app_reconnects_if_the_os_does_not() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.join("Dorm-A");
    sim.send(Event::Sleep);
    sim.wifi.remove_network("Dorm-A");
    sim.wifi.set_network("Dorm-B", -65);
    sim.now += mins(60);
    let woke_at = sim.now;
    sim.send(Event::Wake);

    sim.run(mins(1));
    let switches = sim.switches();
    assert_eq!(switches, [(woke_at + secs(15), "Dorm-B".to_string())]);
}

#[test]
fn wifi_turned_off_does_nothing() {
    let mut sim = Sim::new(&["Dorm-A"]);
    sim.wifi.set_network("Dorm-A", -60);
    sim.wifi.set_powered(false);
    sim.run(mins(5));
    assert!(sim.wifi.calls().is_empty());
}

#[test]
fn a_network_the_user_has_not_added_is_left_alone() {
    let mut sim = Sim::new(&["Dorm-A"]);
    sim.wifi.set_network("Cafe-Free-WiFi", -85);
    sim.wifi.set_network("Dorm-A", -55);
    sim.wifi.join("Cafe-Free-WiFi");
    sim.run(mins(10));
    assert!(sim.wifi.calls().is_empty());
    assert_eq!(sim.connected().as_deref(), Some("Cafe-Free-WiFi"));
}

#[test]
fn a_turned_off_network_is_never_chosen() {
    let mut sim = Sim::with_config(Config {
        networks: vec![
            Network::new("Dorm-A"),
            Network {
                ssid: "Dorm-B".into(),
                enabled: false,
            },
        ],
        ..Config::default()
    });
    sim.wifi.set_network("Dorm-A", -82);
    sim.wifi.set_network("Dorm-B", -50);
    sim.wifi.join("Dorm-A");
    sim.run(mins(10));
    assert!(sim.connect_attempts().is_empty());
    assert_eq!(sim.connected().as_deref(), Some("Dorm-A"));
}

#[test]
fn losing_the_connection_during_a_backoff_reconnects_straight_away() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -80);
    sim.wifi.join("Dorm-A");
    sim.run(secs(20)); // weak, nothing better: backing off until 40 s

    sim.wifi.remove_network("Dorm-A");
    sim.wifi.set_network("Dorm-B", -70);
    let dropped_at = sim.now;
    sim.tick();
    assert_eq!(sim.switches(), [(dropped_at, "Dorm-B".to_string())]);
}

// ----- Internet quality (Phase 4) -----

fn server(addr: &str) -> std::net::IpAddr {
    addr.parse().unwrap()
}

/// Two strong networks, connected to Dorm-A, everything working for a minute.
fn two_strong_networks() -> Sim {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -50);
    sim.wifi.set_network("Dorm-B", -62);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));
    assert!(sim.switches().is_empty());
    sim
}

#[test]
fn strong_signal_but_no_internet_switches_within_30_seconds() {
    let mut sim = two_strong_networks();
    let outage = sim.now;
    sim.wifi.set_internet("Dorm-A", None);
    sim.run(mins(2));

    let switches = sim.switches();
    assert_eq!(switches.len(), 1, "{switches:?}");
    let (at, to) = &switches[0];
    assert_eq!(to, "Dorm-B");
    assert!(*at - outage <= secs(30), "took {:?}", *at - outage);
    assert!(matches!(
        sim.notices[0].1,
        Notice::Switched {
            reason: Reason::BadInternet { .. },
            ..
        }
    ));
}

#[test]
fn slow_internet_switches_within_30_seconds() {
    let mut sim = two_strong_networks();
    let slowdown = sim.now;
    sim.wifi.set_internet("Dorm-A", Some(900));
    sim.run(mins(2));

    let switches = sim.switches();
    assert_eq!(switches.len(), 1, "{switches:?}");
    assert!(
        switches[0].0 - slowdown <= secs(30),
        "took {:?}",
        switches[0].0 - slowdown
    );
}

#[test]
fn one_public_server_down_does_not_switch() {
    let mut sim = two_strong_networks();
    sim.wifi.set_server_down(server("1.1.1.1"), true);
    sim.run(mins(10));
    assert!(sim.switches().is_empty());
    assert_eq!(sim.scans(), 0);
}

#[test]
fn a_network_that_blocks_ping_is_not_called_bad() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -50);
    sim.wifi.set_network("Dorm-B", -62);
    sim.wifi.block_icmp("Dorm-A");
    sim.wifi.join("Dorm-A");
    sim.run(mins(10));

    assert!(sim.switches().is_empty());
    let quality = sim.driver.quality().expect("connected and checking");
    assert!(
        quality.public.iter().all(|target| target.tcp),
        "{quality:?}"
    );
    assert_eq!(quality.ping_ms(), Some(20));
}

#[test]
fn a_router_that_never_answers_is_ignored() {
    let mut sim = Sim::new(&["Dorm-A", "Dorm-B"]);
    sim.wifi.set_network("Dorm-A", -50);
    sim.wifi.set_network("Dorm-B", -62);
    sim.wifi.set_router("Dorm-A", None);
    sim.wifi.join("Dorm-A");
    sim.run(mins(10));
    assert!(sim.switches().is_empty());
}

#[test]
fn a_router_that_stops_answering_is_a_bad_link() {
    let mut sim = two_strong_networks();
    sim.wifi.set_router("Dorm-A", None);
    sim.run(mins(2));
    let switches = sim.switches();
    assert_eq!(switches.len(), 1, "{switches:?}");
    assert!(matches!(
        sim.notices[0].1,
        Notice::Switched {
            reason: Reason::BadLink { .. },
            ..
        }
    ));
}

#[test]
fn bad_internet_everywhere_does_not_ping_pong() {
    let mut sim = two_strong_networks();
    sim.wifi.set_internet("Dorm-A", None);
    sim.wifi.set_internet("Dorm-B", None);
    sim.run(mins(9));
    // Tries Dorm-B once; Dorm-A stays marked for 10 minutes, so no way back.
    assert_eq!(sim.switches().len(), 1, "{:?}", sim.switches());
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));
}

#[test]
fn a_network_left_for_bad_internet_is_skipped_for_ten_minutes() {
    let mut sim = two_strong_networks();
    sim.wifi.set_internet("Dorm-A", None);
    sim.run(mins(1));
    assert_eq!(sim.connected().as_deref(), Some("Dorm-B"));
    let left_at = sim.switches()[0].0;

    // Dorm-A's internet comes back, then Dorm-B's dies.
    sim.wifi.set_internet("Dorm-A", Some(20));
    sim.wifi.set_internet("Dorm-B", None);
    sim.run(mins(15));

    let back = sim
        .switches()
        .into_iter()
        .find(|(_, to)| to == "Dorm-A")
        .expect("goes back to Dorm-A eventually");
    assert!(
        back.0 - left_at >= mins(10),
        "came back after {:?}",
        back.0 - left_at
    );
}

#[test]
fn quality_is_not_checked_while_paused() {
    let mut sim = Sim::with_config(Config {
        auto_switch: false,
        networks: vec![Network::new("Dorm-A")],
        ..Config::default()
    });
    sim.wifi.set_network("Dorm-A", -50);
    sim.wifi.join("Dorm-A");
    sim.run(mins(1));
    assert!(sim.driver.quality().is_none());
}
