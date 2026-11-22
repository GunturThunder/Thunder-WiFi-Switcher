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
    /// Lowest packet loss to the public servers, when connected and checked.
    pub loss_pct: Option<u8>,
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
            loss_pct: None,
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

    /// Adds the latest ping and loss (ignored unless connected).
    pub fn with_quality(mut self, ping_ms: Option<u32>, loss_pct: Option<u8>) -> Self {
        if self.connection == Connection::Connected {
            self.ping_ms = ping_ms;
            self.loss_pct = loss_pct;
        }
        self
    }

    /// The first line of the menu, e.g. `Connected: Dorm-5G · -58 dBm · 24 ms`.
    pub fn summary(&self) -> String {
        match (self.connection, &self.ssid, self.rssi) {
            (Connection::Connected, Some(ssid), Some(rssi)) => {
                let quality = match (self.internet, self.ping_ms) {
                    (InternetState::Bad, ping) => {
                        format!(
                            " · {}",
                            internet_problem(ping, self.loss_pct.unwrap_or(100))
                        )
                    }
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

/// What's wrong with the internet, in plain words: nothing gets through, a
/// lot gets lost, or it gets through slowly. (The median ping only covers
/// probes that answered, so a dying connection can still show a low ping.)
pub fn internet_problem(ping_ms: Option<u32>, loss_pct: u8) -> String {
    match ping_ms {
        None => "no internet".to_owned(),
        Some(_) if loss_pct >= 100 => "no internet".to_owned(),
        Some(_) if loss_pct >= 10 => "internet dropping out".to_owned(),
        Some(ms) => format!("slow internet ({ms} ms)"),
    }
}

/// Why the app left a network with bad internet, for the switch message.
fn internet_reason(from: &str, ping_ms: Option<u32>, loss_pct: u8) -> String {
    match ping_ms {
        None => format!("no internet on {from}"),
        Some(_) if loss_pct >= 100 => format!("no internet on {from}"),
        Some(_) if loss_pct >= 10 => {
            format!("{from}'s internet kept dropping out: {loss_pct}% of checks failed")
        }
        Some(ms) => format!("{from}'s internet was slow: {ms} ms"),
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
        } => format!(
            "Switched from {from} to {to} ({})",
            internet_reason(from, *ping_ms, *loss_pct)
        ),
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

/// A desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// The first line, in bold.
    pub title: String,
    /// The line below it.
    pub body: String,
}

impl Alert {
    /// The notification for an automatic switch (when the user turned them
    /// on). `None` for switches the user made themselves, which they already
    /// know about, and for failed joins, which the menu shows.
    pub fn for_notice(notice: &Notice) -> Option<Self> {
        let Notice::Switched { from, to, reason } = notice else {
            return None;
        };
        let (title, body) = match (reason, from) {
            (Reason::User, _) => return None,
            (Reason::Disconnected, _) => (
                format!("Reconnected to {to}"),
                "The Wi-Fi connection had dropped.".to_owned(),
            ),
            (Reason::WeakSignal { rssi }, Some(from)) => (
                format!("Switched to {to}"),
                format!("{from}'s signal was weak ({rssi} dBm)."),
            ),
            (Reason::WeakSignal { .. }, None) => (
                format!("Switched to {to}"),
                "The signal was weak.".to_owned(),
            ),
            (Reason::BadInternet { ping_ms, loss_pct }, Some(from)) => (
                format!("Switched to {to}"),
                capitalize(&format!("{}.", internet_reason(from, *ping_ms, *loss_pct))),
            ),
            (Reason::BadLink { .. }, Some(from)) => (
                format!("Switched to {to}"),
                format!("{from}'s router stopped responding."),
            ),
            (Reason::BadInternet { .. } | Reason::BadLink { .. }, None) => (
                format!("Switched to {to}"),
                "The internet was bad.".to_owned(),
            ),
        };
        Some(Self { title, body })
    }

    /// Sent once when the user turns notifications on, so they see what one
    /// looks like (and that the system lets them through).
    pub fn turned_on() -> Self {
        Self {
            title: "Notifications are on".to_owned(),
            body: "You'll see one like this when the app switches networks by itself.".to_owned(),
        }
    }
}

/// Upper-cases the first letter ("no internet on A." → "No internet on A.").
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Why joining a network from the settings window failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JoinError {
    /// The kind of problem, so the window can offer the right fix.
    pub kind: JoinProblem,
    /// What went wrong and what to do, in plain words.
    pub message: String,
}

/// The kinds of [`JoinError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinProblem {
    NotInRange,
    NoPassword,
    WrongPassword,
    NeedsUserName,
    Permission,
    WifiOff,
    Timeout,
    Other,
}

impl JoinError {
    /// Explains why joining `ssid` failed, for this computer's OS.
    pub fn new(ssid: &str, error: &Error) -> Self {
        Self::explain(ssid, error, cfg!(windows))
    }

    fn explain(ssid: &str, error: &Error, windows: bool) -> Self {
        use JoinProblem as P;
        let (kind, message) = match error {
            Error::NotFound => (
                P::NotInRange,
                format!(
                    "{ssid} isn't in range right now. Move closer to its router, or check that \
                     the name is spelled exactly right."
                ),
            ),
            Error::NoPassword if windows => (
                P::NoPassword,
                format!(
                    "Windows hasn't saved a password for {ssid}. Connect to it once from the \
                     Windows Wi-Fi menu, then try again."
                ),
            ),
            Error::NoPassword => (
                P::NoPassword,
                format!("No password is saved for {ssid}. Add it, then try again."),
            ),
            Error::AuthFailed if windows => (
                P::WrongPassword,
                format!(
                    "{ssid} didn't accept the connection. If its password changed, forget it in \
                     Windows Wi-Fi settings and connect again from the Wi-Fi menu."
                ),
            ),
            Error::AuthFailed => (
                P::WrongPassword,
                format!(
                    "{ssid} didn't accept the connection. The saved password may be wrong: \
                     change it, then try again."
                ),
            ),
            Error::NeedsUserName => (
                P::NeedsUserName,
                format!(
                    "{ssid} needs a user name as well as a password (WPA Enterprise). The app \
                     can't join these yet: join it from the macOS Wi-Fi menu."
                ),
            ),
            Error::PermissionDenied if windows => (
                P::Permission,
                "Windows hides Wi-Fi networks while location is off for desktop apps. Turn it \
                 on, then try again."
                    .to_owned(),
            ),
            Error::PermissionDenied => (
                P::Permission,
                "macOS only shows Wi-Fi networks to apps with Location permission. Allow it, \
                 then try again."
                    .to_owned(),
            ),
            Error::WifiOff => (
                P::WifiOff,
                "Wi-Fi is turned off. Turn it on, then try again.".to_owned(),
            ),
            Error::Timeout => (
                P::Timeout,
                format!(
                    "{ssid} didn't answer in time. It may be too far away or busy: try again in \
                     a moment."
                ),
            ),
            Error::Os(message) => (P::Other, format!("Couldn't join {ssid}: {message}")),
        };
        Self { kind, message }
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
        let status = status(Ok(link("Dorm-5G", -58))).with_quality(Some(24), Some(0));
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
        let status = status_of(link_with("Dorm-5G", -50, slow)).with_quality(Some(900), Some(0));
        assert_eq!(
            status.summary(),
            "Connected: Dorm-5G · -50 dBm · slow internet (900 ms)"
        );

        // Just went down: the last answers were quick, but half were lost.
        let failing = Internet::Bad {
            ping_ms: Some(24),
            loss_pct: 50,
        };
        let status = status_of(link_with("Dorm-5G", -50, failing)).with_quality(Some(24), Some(50));
        assert_eq!(
            status.summary(),
            "Connected: Dorm-5G · -50 dBm · internet dropping out"
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
        let status = status(Ok(Sample::Disconnected)).with_quality(Some(24), Some(0));
        assert_eq!(status.ping_ms, None);
        assert_eq!(status.loss_pct, None);
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
            "Switched from A to B (A's internet kept dropping out: 40% of checks failed)"
        );
        assert_eq!(
            notice_text(&switched(
                Some("A"),
                Reason::BadInternet {
                    ping_ms: Some(900),
                    loss_pct: 0
                }
            )),
            "Switched from A to B (A's internet was slow: 900 ms)"
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

    #[test]
    fn notifications_only_for_automatic_switches() {
        let switched = |from: Option<&str>, reason| Notice::Switched {
            from: from.map(Into::into),
            to: "B".into(),
            reason,
        };
        let alert = |notice| Alert::for_notice(&notice).map(|a| (a.title, a.body));

        assert_eq!(
            alert(switched(Some("A"), Reason::WeakSignal { rssi: -81 })),
            Some((
                "Switched to B".into(),
                "A's signal was weak (-81 dBm).".into()
            ))
        );
        assert_eq!(
            alert(switched(None, Reason::Disconnected)),
            Some((
                "Reconnected to B".into(),
                "The Wi-Fi connection had dropped.".into()
            ))
        );
        assert_eq!(
            alert(switched(
                Some("A"),
                Reason::BadInternet {
                    ping_ms: None,
                    loss_pct: 100
                }
            )),
            Some(("Switched to B".into(), "No internet on A.".into()))
        );
        assert_eq!(
            alert(switched(
                Some("A"),
                Reason::BadLink {
                    ping_ms: None,
                    loss_pct: 100
                }
            )),
            Some((
                "Switched to B".into(),
                "A's router stopped responding.".into()
            ))
        );
        // The user did it themselves, or nothing changed.
        assert_eq!(alert(switched(Some("A"), Reason::User)), None);
        assert_eq!(
            alert(Notice::ConnectFailed {
                ssid: "B".into(),
                error: Error::AuthFailed
            }),
            None
        );
    }

    #[test]
    fn join_errors_say_what_to_do_on_each_os() {
        let mac = |error| JoinError::explain("Kos-2", &error, false);
        let windows = |error| JoinError::explain("Kos-2", &error, true);

        assert_eq!(mac(Error::NotFound).kind, JoinProblem::NotInRange);
        assert!(
            mac(Error::NotFound)
                .message
                .starts_with("Kos-2 isn't in range")
        );

        assert_eq!(mac(Error::NoPassword).kind, JoinProblem::NoPassword);
        assert!(mac(Error::NoPassword).message.contains("Add it"));
        assert!(
            windows(Error::NoPassword)
                .message
                .contains("Windows Wi-Fi menu")
        );

        assert_eq!(mac(Error::AuthFailed).kind, JoinProblem::WrongPassword);
        assert!(mac(Error::AuthFailed).message.contains("change it"));
        assert!(
            windows(Error::AuthFailed)
                .message
                .contains("Windows Wi-Fi settings")
        );

        assert_eq!(mac(Error::NeedsUserName).kind, JoinProblem::NeedsUserName);
        assert_eq!(mac(Error::PermissionDenied).kind, JoinProblem::Permission);
        assert!(
            mac(Error::PermissionDenied)
                .message
                .contains("Location permission")
        );
        assert!(
            windows(Error::PermissionDenied)
                .message
                .contains("location is off")
        );
        assert_eq!(mac(Error::WifiOff).kind, JoinProblem::WifiOff);
        assert_eq!(mac(Error::Timeout).kind, JoinProblem::Timeout);

        let other = mac(Error::Os("Resource busy".into()));
        assert_eq!(other.kind, JoinProblem::Other);
        assert_eq!(other.message, "Couldn't join Kos-2: Resource busy");
    }
}
