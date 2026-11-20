//! What the menu bar and settings window show, built from the engine's state
//! and the latest reading of the Wi-Fi link.

use serde::Serialize;
use thunder_core::{
    Engine, Error, Internet, Notice, PauseReason, Permission, Reason, Sample, State,
};

/// The Wi-Fi connection right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    /// The app hasn't read the Wi-Fi state yet.
    Starting,
    /// The Wi-Fi radio is off.
    Off,
    /// Wi-Fi is on but not connected.
    Disconnected,
    /// Connected to a network.
    Connected,
    /// macOS hides network names until Location permission is granted.
    PermissionNeeded,
    /// Wi-Fi can't be read (see `error`).
    Unavailable,
}

/// What the engine is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// Checking the signal every 5 seconds.
    Watching,
    /// Looking for a better network.
    Scanning,
    /// Joining a network.
    Switching,
    /// Just switched or woke up; giving the network a moment.
    Settling,
    /// Nothing better was found; waiting before trying again.
    WaitingToRetry,
    /// The user turned auto-switch off.
    Paused,
    /// The computer is asleep.
    Asleep,
}

/// What the internet checks say about the current network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InternetState {
    /// Not judged yet, or not connected, or checks paused.
    Unknown,
    /// Works.
    Good,
    /// Both public servers slow or unreachable.
    Bad,
    /// The router is slow or not answering.
    BadLink,
}

impl From<Internet> for InternetState {
    fn from(internet: Internet) -> Self {
        match internet {
            Internet::Unknown => Self::Unknown,
            Internet::Good => Self::Good,
            Internet::Bad { .. } => Self::Bad,
            Internet::BadLink { .. } => Self::BadLink,
        }
    }
}

/// Location permission, for the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// Granted.
    Granted,
    /// Refused or turned off.
    Denied,
    /// Not asked yet.
    NotDetermined,
}

impl From<Permission> for PermissionState {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::Granted => Self::Granted,
            Permission::Denied => Self::Denied,
            Permission::NotDetermined => Self::NotDetermined,
        }
    }
}

/// The last thing the app did, for the menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastEvent {
    /// Local time, e.g. `15:42`.
    pub time: String,
    /// What happened, in plain words.
    pub text: String,
}

impl LastEvent {
    /// An event that happened just now.
    pub fn now(notice: &Notice) -> Self {
        Self {
            time: chrono::Local::now().format("%H:%M").to_string(),
            text: notice_text(notice),
        }
    }
}

/// Everything the menu bar and settings window show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The connection right now.
    pub connection: Connection,
    /// Current network, when connected.
    pub ssid: Option<String>,
    /// Current signal (dBm), when connected.
    pub rssi: Option<i32>,
    /// Best median ping to the public servers, when connected and checked.
    pub ping_ms: Option<u32>,
    /// Whether the internet behind the network works.
    pub internet: InternetState,
    /// Whether the current network is one of the user's (enabled) networks.
    pub managed: bool,
    /// What the engine is doing.
    pub activity: Activity,
    /// Location permission.
    pub permission: PermissionState,
    /// The last switch or failed switch.
    pub last_event: Option<LastEvent>,
    /// Why Wi-Fi can't be read, when `connection` is `Unavailable`.
    pub error: Option<String>,
    /// Running with pretend networks (`--simulate`).
    pub simulated: bool,
}

impl Status {
    /// Builds the status from the engine and the latest reading (`None`
    /// before the first one).
    pub fn new(
        engine: &Engine,
        reading: Option<&Result<Sample, Error>>,
        permission: Permission,
        last_event: Option<LastEvent>,
        simulated: bool,
    ) -> Self {
        let mut status = Self {
            connection: Connection::Starting,
            ssid: None,
            rssi: None,
            ping_ms: None,
            internet: InternetState::Unknown,
            managed: false,
            activity: activity(engine.state()),
            permission: permission.into(),
            last_event,
            error: None,
            simulated,
        };
        match reading {
            None => {}
            Some(Ok(Sample::WifiOff)) => status.connection = Connection::Off,
            Some(Ok(Sample::Disconnected)) => status.connection = Connection::Disconnected,
            Some(Ok(Sample::Connected(link, internet))) => {
                status.connection = Connection::Connected;
                status.managed = engine.config().is_enabled(&link.ssid);
                status.ssid = Some(link.ssid.clone());
                status.rssi = Some(link.rssi);
                status.internet = (*internet).into();
            }
            Some(Err(Error::PermissionDenied)) => status.connection = Connection::PermissionNeeded,
            Some(Err(err)) => {
                status.connection = Connection::Unavailable;
                status.error = Some(err.to_string());
            }
        }
        status
    }

    /// Adds the latest ping (ignored unless connected).
    pub fn with_ping(mut self, ping_ms: Option<u32>) -> Self {
        if self.connection == Connection::Connected {
            self.ping_ms = ping_ms;
        }
        self
    }

    /// The first line of the menu, e.g. `Connected: Dorm-5G · -58 dBm · 24 ms`.
    pub fn summary(&self) -> String {
        match (self.connection, &self.ssid, self.rssi) {
            (Connection::Connected, Some(ssid), Some(rssi)) => {
                let quality = match (self.internet, self.ping_ms) {
                    (InternetState::Bad, None) => " · no internet".to_owned(),
                    (InternetState::Bad, Some(_)) => " · slow internet".to_owned(),
                    (InternetState::BadLink, _) => " · router not responding".to_owned(),
                    (_, Some(ms)) => format!(" · {ms} ms"),
                    (_, None) => String::new(),
                };
                format!("Connected: {ssid} · {rssi} dBm{quality}")
            }
            (Connection::Connected, _, _) => "Connected".into(),
            (Connection::Starting, _, _) => "Starting…".into(),
            (Connection::Off, _, _) => "Wi-Fi is off".into(),
            (Connection::Disconnected, _, _) => "Not connected".into(),
            (Connection::PermissionNeeded, _, _) => "Needs Location permission".into(),
            (Connection::Unavailable, _, _) => self
                .error
                .clone()
                .unwrap_or_else(|| "Wi-Fi unavailable".into()),
        }
    }

    /// Short text next to the menu bar icon, e.g. `-58 · 24ms`.
    pub fn label(&self) -> Option<String> {
        match (self.connection, self.rssi, self.ping_ms) {
            (Connection::Connected, Some(rssi), Some(ms)) => Some(format!("{rssi} · {ms}ms")),
            (Connection::Connected, Some(rssi), None) => Some(rssi.to_string()),
            _ => None,
        }
    }

    /// How many arcs the menu bar icon shows (0–3), or `None` for the
    /// crossed-out icon.
    pub fn bars(&self) -> Option<u8> {
        match (self.connection, self.rssi) {
            (Connection::Connected, Some(rssi)) => Some(signal_bars(rssi)),
            _ => None,
        }
    }
}

/// Arcs for a signal level, roughly matching the system Wi-Fi icon.
pub fn signal_bars(rssi: i32) -> u8 {
    match rssi {
        r if r >= -60 => 3,
        r if r >= -70 => 2,
        r if r >= -80 => 1,
        _ => 0,
    }
}

fn activity(state: &State) -> Activity {
    match state {
        State::Watching { .. } => Activity::Watching,
        State::Scanning { .. } => Activity::Scanning,
        State::Switching { .. } => Activity::Switching,
        State::Cooldown { .. } => Activity::Settling,
        State::Backoff { .. } => Activity::WaitingToRetry,
        State::Paused(PauseReason::User) => Activity::Paused,
        State::Paused(PauseReason::Sleep) => Activity::Asleep,
    }
}

/// A notice in plain words, for the menu and the log.
pub fn notice_text(notice: &Notice) -> String {
    match notice {
        Notice::Switched {
            from: Some(from),
            to,
            reason: Reason::WeakSignal { rssi },
        } => format!("Switched from {from} to {to} (signal was {rssi} dBm)"),
        Notice::Switched {
            to,
            reason: Reason::WeakSignal { .. },
            ..
        } => format!("Switched to {to} (weak signal)"),
        Notice::Switched {
            to,
            reason: Reason::Disconnected,
            ..
        } => format!("Reconnected to {to}"),
        Notice::Switched {
            to,
            reason: Reason::User,
            ..
        } => format!("Switched to {to}"),
        Notice::Switched {
            from: Some(from),
            to,
            reason: Reason::BadInternet { ping_ms, loss_pct },
        } => match ping_ms {
            Some(ms) if *loss_pct < 100 => format!(
                "Switched from {from} to {to} (internet on {from} was slow: {ms} ms, {loss_pct}% loss)"
            ),
            _ => format!("Switched from {from} to {to} (no internet on {from})"),
        },
        Notice::Switched {
            from: Some(from),
            to,
            reason: Reason::BadLink { .. },
        } => format!("Switched from {from} to {to} ({from}'s router stopped responding)"),
        Notice::Switched {
            to,
            reason: Reason::BadInternet { .. } | Reason::BadLink { .. },
            ..
        } => format!("Switched to {to} (bad internet)"),
        Notice::ConnectFailed { ssid, error } => format!("Couldn't join {ssid}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunder_core::{Config, Link, Network};

    fn engine() -> Engine {
        Engine::new(Config {
            networks: vec![Network::new("Dorm-5G")],
            ..Config::default()
        })
    }

    fn link(ssid: &str, rssi: i32) -> Sample {
        link_with(ssid, rssi, Internet::Good)
    }

    fn link_with(ssid: &str, rssi: i32, internet: Internet) -> Sample {
        Sample::Connected(
            Link {
                ssid: ssid.into(),
                bssid: None,
                rssi,
                channel: None,
            },
            internet,
        )
    }

    #[test]
    fn ping_shows_in_the_menu_and_label() {
        let status = status(Ok(link("Dorm-5G", -58))).with_ping(Some(24));
        assert_eq!(status.summary(), "Connected: Dorm-5G · -58 dBm · 24 ms");
        assert_eq!(status.label().as_deref(), Some("-58 · 24ms"));
        assert_eq!(status.internet, InternetState::Good);
    }

    #[test]
    fn bad_internet_shows_in_the_menu() {
        let down = Internet::Bad {
            ping_ms: None,
            loss_pct: 100,
        };
        let status = status(Ok(link_with("Dorm-5G", -50, down)));
        assert_eq!(
            status.summary(),
            "Connected: Dorm-5G · -50 dBm · no internet"
        );
        assert_eq!(status.internet, InternetState::Bad);

        let slow = Internet::Bad {
            ping_ms: Some(900),
            loss_pct: 0,
        };
        let status = status_of(link_with("Dorm-5G", -50, slow)).with_ping(Some(900));
        assert_eq!(
            status.summary(),
            "Connected: Dorm-5G · -50 dBm · slow internet"
        );

        let router = Internet::BadLink {
            ping_ms: None,
            loss_pct: 100,
        };
        let status = status_of(link_with("Dorm-5G", -50, router));
        assert_eq!(
            status.summary(),
            "Connected: Dorm-5G · -50 dBm · router not responding"
        );
    }

    fn status_of(sample: Sample) -> Status {
        status(Ok(sample))
    }

    #[test]
    fn ping_is_dropped_when_not_connected() {
        let status = status(Ok(Sample::Disconnected)).with_ping(Some(24));
        assert_eq!(status.ping_ms, None);
    }

    fn status(reading: Result<Sample, Error>) -> Status {
        Status::new(&engine(), Some(&reading), Permission::Granted, None, false)
    }

    #[test]
    fn connected_to_a_managed_network() {
        let status = status(Ok(link("Dorm-5G", -58)));
        assert_eq!(status.summary(), "Connected: Dorm-5G · -58 dBm");
        assert_eq!(status.label().as_deref(), Some("-58"));
        assert_eq!(status.bars(), Some(3));
        assert!(status.managed);
        assert_eq!(status.activity, Activity::Watching);
    }

    #[test]
    fn connected_to_someone_elses_network() {
        let status = status(Ok(link("Cafe", -70)));
        assert!(!status.managed);
        assert_eq!(status.bars(), Some(2));
    }

    #[test]
    fn every_other_connection_state_has_a_summary_and_no_bars() {
        let cases = [
            (Ok(Sample::WifiOff), "Wi-Fi is off"),
            (Ok(Sample::Disconnected), "Not connected"),
            (Err(Error::PermissionDenied), "Needs Location permission"),
            (Err(Error::Os("No Wi-Fi".into())), "Wi-Fi error: No Wi-Fi"),
        ];
        for (reading, summary) in cases {
            let status = status(reading);
            assert_eq!(status.summary(), summary);
            assert_eq!(status.bars(), None);
            assert_eq!(status.label(), None);
        }
        let starting = Status::new(&engine(), None, Permission::Granted, None, false);
        assert_eq!(starting.summary(), "Starting…");
    }

    #[test]
    fn signal_bars_by_strength() {
        assert_eq!(signal_bars(-45), 3);
        assert_eq!(signal_bars(-60), 3);
        assert_eq!(signal_bars(-61), 2);
        assert_eq!(signal_bars(-75), 1);
        assert_eq!(signal_bars(-80), 1);
        assert_eq!(signal_bars(-81), 0);
    }

    #[test]
    fn notices_in_plain_words() {
        let switched = |from: Option<&str>, reason| Notice::Switched {
            from: from.map(Into::into),
            to: "B".into(),
            reason,
        };
        assert_eq!(
            notice_text(&switched(Some("A"), Reason::WeakSignal { rssi: -81 })),
            "Switched from A to B (signal was -81 dBm)"
        );
        assert_eq!(
            notice_text(&switched(None, Reason::Disconnected)),
            "Reconnected to B"
        );
        assert_eq!(
            notice_text(&switched(Some("A"), Reason::User)),
            "Switched to B"
        );
        assert_eq!(
            notice_text(&switched(
                Some("A"),
                Reason::BadInternet {
                    ping_ms: None,
                    loss_pct: 100
                }
            )),
            "Switched from A to B (no internet on A)"
        );
        assert_eq!(
            notice_text(&switched(
                Some("A"),
                Reason::BadInternet {
                    ping_ms: Some(850),
                    loss_pct: 40
                }
            )),
            "Switched from A to B (internet on A was slow: 850 ms, 40% loss)"
        );
        assert_eq!(
            notice_text(&switched(
                Some("A"),
                Reason::BadLink {
                    ping_ms: None,
                    loss_pct: 100
                }
            )),
            "Switched from A to B (A's router stopped responding)"
        );
        assert_eq!(
            notice_text(&Notice::ConnectFailed {
                ssid: "B".into(),
                error: Error::AuthFailed
            }),
            "Couldn't join B: the network rejected the connection (wrong password?)"
        );
    }
}
