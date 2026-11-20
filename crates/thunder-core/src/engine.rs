//! The switching engine: a state machine that turns samples and results into
//! actions.
//!
//! [`Engine::step`] reads no clock and touches no OS. The caller passes in the
//! time and what just happened ([`Event`]), and gets back what to do next
//! ([`Action`]). That makes every rule testable with scripted events; see
//! [`crate::driver::Driver`] for the loop that runs it against a real backend.

use std::collections::HashMap;
use std::time::Duration;

use crate::config::Config;
use crate::rules::{
    self, AVOID_AFTER_FAILED_CONNECT, Assessment, BACKOFF_MAX, BACKOFF_START,
    BAD_SAMPLES_BEFORE_SWITCH, COOLDOWN_AFTER_SWITCH, STUCK_TIMEOUT, WAKE_GRACE,
};
use crate::wifi::{Error, Link, ScanResult};

/// One reading of the Wi-Fi state, taken every [`rules::SAMPLE_INTERVAL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sample {
    /// The Wi-Fi radio is off.
    WifiOff,
    /// Wi-Fi is on but not connected to any network.
    Disconnected,
    /// Connected to this network.
    Connected(Link),
}

/// Something that happened, fed into [`Engine::step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A new reading of the current link.
    Sample(Sample),
    /// The scan asked for by [`Action::Scan`] finished.
    ScanDone(Result<Vec<ScanResult>, Error>),
    /// The connection asked for by [`Action::Connect`] finished.
    ConnectDone {
        /// The network that was joined.
        ssid: String,
        /// Whether joining worked.
        result: Result<(), Error>,
    },
    /// The laptop is going to sleep.
    Sleep,
    /// The laptop woke up.
    Wake,
    /// The user picked a network (the tray's "Switch to" menu). Works even
    /// while auto-switch is paused.
    UserSwitch {
        /// The network to join.
        ssid: String,
    },
}

/// What the engine wants done next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do.
    None,
    /// Scan for networks, then send [`Event::ScanDone`].
    Scan,
    /// Join this network, then send [`Event::ConnectDone`].
    Connect {
        /// The network to join.
        ssid: String,
    },
    /// Tell the user something (tray "last event", and a notification if
    /// they turned those on).
    Notify(Notice),
}

/// Something worth telling the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// Moved to another network.
    Switched {
        /// The network it left, or `None` if the connection had dropped.
        from: Option<String>,
        /// The network it joined.
        to: String,
        /// Why it moved.
        reason: Reason,
    },
    /// Tried to join a network and couldn't. It's skipped for
    /// [`rules::AVOID_AFTER_FAILED_CONNECT`].
    ConnectFailed {
        /// The network that failed.
        ssid: String,
        /// What went wrong.
        error: Error,
    },
}

/// Why the engine is looking for another network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The signal stayed below the weak-signal level.
    WeakSignal {
        /// Signal (dBm) when the search started.
        rssi: i32,
    },
    /// The connection dropped.
    Disconnected,
    /// The user asked for it.
    User,
}

/// Why auto-switching is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// The user turned auto-switch off.
    User,
    /// The laptop is asleep.
    Sleep,
}

/// Where the engine is. See `docs/TECHNICAL-DESIGN.md` for the diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Checking samples. Counts bad samples in a row.
    Watching {
        /// Bad samples in a row so far.
        bad_streak: u32,
    },
    /// Waiting for a scan.
    Scanning {
        /// Why it's looking.
        reason: Reason,
        /// The link when the search started (`None` if disconnected).
        from: Option<Link>,
        /// When the scan was asked for.
        since: Duration,
    },
    /// Waiting for a connection attempt.
    Switching {
        /// The network being joined.
        to: String,
        /// The network it's leaving, if any.
        from: Option<String>,
        /// Why it's moving.
        reason: Reason,
        /// When the connection was asked for.
        since: Duration,
    },
    /// Just switched or woke up; ignoring samples until `until`.
    Cooldown {
        /// When it starts watching again.
        until: Duration,
    },
    /// Nothing better was found (or joining failed); waiting before retrying.
    Backoff {
        /// The problem it's waiting out.
        reason: Reason,
        /// When it may retry.
        until: Duration,
    },
    /// Auto-switch is off, or the laptop is asleep.
    Paused(PauseReason),
}

const WATCHING: State = State::Watching { bad_streak: 0 };

/// The switching engine. Create one per app run and feed it events.
#[derive(Debug, Clone)]
pub struct Engine {
    config: Config,
    state: State,
    last_sample: Option<Sample>,
    next_backoff: Duration,
    avoid_until: HashMap<String, Duration>,
    asleep: bool,
}

impl Engine {
    /// Starts watching, or paused if `config.auto_switch` is off.
    pub fn new(config: Config) -> Self {
        let mut engine = Self {
            config,
            state: WATCHING,
            last_sample: None,
            next_backoff: BACKOFF_START,
            avoid_until: HashMap::new(),
            asleep: false,
        };
        engine.update_pause(None);
        engine
    }

    /// Current state, for the tray and settings window.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Settings the engine is using.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The most recent sample, for showing signal in the tray.
    pub fn last_sample(&self) -> Option<&Sample> {
        self.last_sample.as_ref()
    }

    /// Networks skipped after a failed connection, with when they're allowed
    /// again.
    pub fn avoided(&self) -> impl Iterator<Item = (&str, Duration)> {
        self.avoid_until
            .iter()
            .map(|(ssid, until)| (ssid.as_str(), *until))
    }

    /// Applies new settings (network list, threshold, auto-switch on/off).
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
        if let State::Watching { .. } = self.state {
            // The threshold may have changed, so start counting again.
            self.state = WATCHING;
        }
        self.update_pause(None);
    }

    /// Handles one event and says what to do next. `now` is any monotonic
    /// time (e.g. time since the app started); only differences matter.
    pub fn step(&mut self, now: Duration, event: Event) -> Action {
        self.avoid_until.retain(|_, until| *until > now);
        match event {
            Event::Sample(sample) => self.on_sample(now, sample),
            Event::ScanDone(result) => self.on_scan_done(now, result),
            Event::ConnectDone { ssid, result } => self.on_connect_done(now, &ssid, result),
            Event::UserSwitch { ssid } => self.on_user_switch(now, ssid),
            Event::Sleep => {
                self.asleep = true;
                self.update_pause(Some(now));
                Action::None
            }
            Event::Wake => {
                self.asleep = false;
                self.update_pause(Some(now));
                Action::None
            }
        }
    }

    fn on_sample(&mut self, now: Duration, sample: Sample) -> Action {
        let assessment = rules::assess(&sample, &self.config);
        if assessment == Assessment::Fine {
            // Back on a good network: the next retry starts short again.
            self.next_backoff = BACKOFF_START;
        }
        self.last_sample = Some(sample);

        if let State::Cooldown { until } = self.state
            && now >= until
        {
            self.state = WATCHING;
        }

        match self.state.clone() {
            State::Paused(_) | State::Cooldown { .. } => Action::None,

            State::Scanning { reason, since, .. } | State::Switching { reason, since, .. } => {
                // The worker never answered; don't wait forever.
                if now.saturating_sub(since) >= STUCK_TIMEOUT {
                    self.enter_backoff(now, reason);
                }
                Action::None
            }

            State::Watching { bad_streak } => match assessment {
                Assessment::Fine | Assessment::HandsOff => {
                    self.state = WATCHING;
                    Action::None
                }
                // A dropped connection doesn't wait for 3 samples.
                Assessment::Problem(Reason::Disconnected) => {
                    self.start_scan(now, Reason::Disconnected)
                }
                Assessment::Problem(reason) => {
                    let bad_streak = bad_streak + 1;
                    if bad_streak >= BAD_SAMPLES_BEFORE_SWITCH {
                        self.start_scan(now, reason)
                    } else {
                        self.state = State::Watching { bad_streak };
                        Action::None
                    }
                }
            },

            State::Backoff {
                reason: waiting_for,
                until,
            } => match assessment {
                Assessment::Fine | Assessment::HandsOff => {
                    self.state = WATCHING;
                    Action::None
                }
                // Backing off from a weak signal, then losing the connection
                // altogether, is a new problem: reconnect now.
                Assessment::Problem(Reason::Disconnected)
                    if waiting_for != Reason::Disconnected =>
                {
                    self.start_scan(now, Reason::Disconnected)
                }
                Assessment::Problem(reason) if now >= until => self.start_scan(now, reason),
                Assessment::Problem(_) => Action::None,
            },
        }
    }

    fn on_scan_done(&mut self, now: Duration, result: Result<Vec<ScanResult>, Error>) -> Action {
        // Results that arrive after a pause or timeout are stale.
        let State::Scanning { reason, from, .. } = self.state.clone() else {
            return Action::None;
        };
        let choice = result.ok().and_then(|scan| {
            rules::choose_network(&scan, &self.config.networks, from.as_ref(), |ssid| {
                self.avoid_until.contains_key(ssid)
            })
        });
        match choice {
            Some(ssid) => {
                self.state = State::Switching {
                    to: ssid.clone(),
                    from: from.map(|link| link.ssid),
                    reason,
                    since: now,
                };
                Action::Connect { ssid }
            }
            None => {
                self.enter_backoff(now, reason);
                Action::None
            }
        }
    }

    fn on_connect_done(&mut self, now: Duration, ssid: &str, result: Result<(), Error>) -> Action {
        let State::Switching {
            to, from, reason, ..
        } = self.state.clone()
        else {
            return Action::None;
        };
        if to != ssid {
            return Action::None;
        }
        match result {
            Ok(()) => {
                self.state = State::Cooldown {
                    until: now + COOLDOWN_AFTER_SWITCH,
                };
                // A switch the user asked for while paused stays paused.
                self.update_pause(None);
                Action::Notify(Notice::Switched { from, to, reason })
            }
            Err(error) => {
                self.avoid_until
                    .insert(to.clone(), now + AVOID_AFTER_FAILED_CONNECT);
                if reason == Reason::User {
                    // The user can simply try again; no need to wait.
                    self.state = WATCHING;
                    self.update_pause(None);
                } else {
                    self.enter_backoff(now, reason);
                }
                Action::Notify(Notice::ConnectFailed { ssid: to, error })
            }
        }
    }

    fn on_user_switch(&mut self, now: Duration, ssid: String) -> Action {
        let from = match &self.last_sample {
            Some(Sample::Connected(link)) => Some(link.ssid.clone()),
            _ => None,
        };
        if from.as_deref() == Some(ssid.as_str()) {
            return Action::None;
        }
        // Takes over from whatever was happening; late scan or connect
        // results for that are ignored.
        self.state = State::Switching {
            to: ssid.clone(),
            from,
            reason: Reason::User,
            since: now,
        };
        Action::Connect { ssid }
    }

    fn start_scan(&mut self, now: Duration, reason: Reason) -> Action {
        let from = match (&reason, &self.last_sample) {
            (Reason::WeakSignal { .. }, Some(Sample::Connected(link))) => Some(link.clone()),
            _ => None,
        };
        self.state = State::Scanning {
            reason,
            from,
            since: now,
        };
        Action::Scan
    }

    fn enter_backoff(&mut self, now: Duration, reason: Reason) {
        self.state = State::Backoff {
            reason,
            until: now + self.next_backoff,
        };
        self.next_backoff = (self.next_backoff * 2).min(BACKOFF_MAX);
    }

    /// Moves into or out of [`State::Paused`]. `now` is set when waking, so
    /// the OS gets [`WAKE_GRACE`] to rejoin a network before the engine acts.
    fn update_pause(&mut self, now: Option<Duration>) {
        let pause = if !self.config.auto_switch {
            Some(PauseReason::User)
        } else if self.asleep {
            Some(PauseReason::Sleep)
        } else {
            None
        };
        match (pause, &self.state) {
            (Some(reason), _) => self.state = State::Paused(reason),
            (None, State::Paused(PauseReason::Sleep)) => {
                self.state = match now {
                    Some(now) => State::Cooldown {
                        until: now + WAKE_GRACE,
                    },
                    None => WATCHING,
                };
            }
            (None, State::Paused(PauseReason::User)) => self.state = WATCHING,
            (None, _) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Network;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn config() -> Config {
        Config {
            networks: vec![Network::new("A"), Network::new("B")],
            ..Config::default()
        }
    }

    fn connected(ssid: &str, rssi: i32) -> Event {
        Event::Sample(Sample::Connected(Link {
            ssid: ssid.into(),
            bssid: None,
            rssi,
            channel: None,
        }))
    }

    fn seen(ssid: &str, rssi: i32) -> ScanResult {
        ScanResult {
            ssid: ssid.into(),
            bssid: None,
            rssi,
            channel: None,
        }
    }

    /// Feeds three weak samples so the engine asks for a scan.
    fn engine_scanning() -> Engine {
        let mut engine = Engine::new(config());
        assert_eq!(engine.step(secs(0), connected("A", -80)), Action::None);
        assert_eq!(engine.step(secs(5), connected("A", -80)), Action::None);
        assert_eq!(engine.step(secs(10), connected("A", -80)), Action::Scan);
        engine
    }

    #[test]
    fn three_weak_samples_in_a_row_start_a_scan() {
        let engine = engine_scanning();
        assert!(matches!(
            engine.state(),
            State::Scanning {
                reason: Reason::WeakSignal { rssi: -80 },
                ..
            }
        ));
    }

    #[test]
    fn a_good_sample_resets_the_count() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), connected("A", -80));
        engine.step(secs(5), connected("A", -80));
        engine.step(secs(10), connected("A", -60));
        assert_eq!(engine.step(secs(15), connected("A", -80)), Action::None);
        assert_eq!(engine.state(), &State::Watching { bad_streak: 1 });
    }

    #[test]
    fn disconnect_scans_straight_away() {
        let mut engine = Engine::new(config());
        assert_eq!(
            engine.step(secs(0), Event::Sample(Sample::Disconnected)),
            Action::Scan
        );
    }

    #[test]
    fn scan_with_a_winner_connects_then_cools_down() {
        let mut engine = engine_scanning();
        let action = engine.step(secs(12), Event::ScanDone(Ok(vec![seen("B", -60)])));
        assert_eq!(action, Action::Connect { ssid: "B".into() });

        let action = engine.step(
            secs(14),
            Event::ConnectDone {
                ssid: "B".into(),
                result: Ok(()),
            },
        );
        assert_eq!(
            action,
            Action::Notify(Notice::Switched {
                from: Some("A".into()),
                to: "B".into(),
                reason: Reason::WeakSignal { rssi: -80 },
            })
        );
        assert_eq!(engine.state(), &State::Cooldown { until: secs(44) });
        // Samples during the cooldown are ignored, even bad ones.
        assert_eq!(engine.step(secs(20), connected("B", -90)), Action::None);
    }

    #[test]
    fn failed_scan_backs_off() {
        let mut engine = engine_scanning();
        let action = engine.step(secs(12), Event::ScanDone(Err(Error::Timeout)));
        assert_eq!(action, Action::None);
        assert!(matches!(engine.state(), State::Backoff { until, .. } if *until == secs(42)));
    }

    #[test]
    fn failed_connect_avoids_that_network_and_backs_off() {
        let mut engine = engine_scanning();
        engine.step(secs(12), Event::ScanDone(Ok(vec![seen("B", -60)])));
        let action = engine.step(
            secs(13),
            Event::ConnectDone {
                ssid: "B".into(),
                result: Err(Error::AuthFailed),
            },
        );
        assert_eq!(
            action,
            Action::Notify(Notice::ConnectFailed {
                ssid: "B".into(),
                error: Error::AuthFailed,
            })
        );
        assert!(matches!(engine.state(), State::Backoff { .. }));
        assert_eq!(engine.avoided().collect::<Vec<_>>(), [("B", secs(613))]);
    }

    #[test]
    fn avoided_networks_come_back_after_ten_minutes() {
        let mut engine = engine_scanning();
        engine.step(secs(12), Event::ScanDone(Ok(vec![seen("B", -60)])));
        engine.step(
            secs(13),
            Event::ConnectDone {
                ssid: "B".into(),
                result: Err(Error::AuthFailed),
            },
        );
        engine.step(secs(613), connected("A", -80));
        assert_eq!(engine.avoided().count(), 0);
    }

    #[test]
    fn stale_results_are_ignored() {
        let mut engine = Engine::new(config());
        let before = engine.state().clone();
        assert_eq!(
            engine.step(secs(0), Event::ScanDone(Ok(vec![seen("B", -40)]))),
            Action::None
        );
        assert_eq!(
            engine.step(
                secs(0),
                Event::ConnectDone {
                    ssid: "B".into(),
                    result: Ok(())
                }
            ),
            Action::None
        );
        assert_eq!(engine.state(), &before);
    }

    #[test]
    fn result_for_a_different_network_is_ignored() {
        let mut engine = engine_scanning();
        engine.step(secs(12), Event::ScanDone(Ok(vec![seen("B", -60)])));
        let action = engine.step(
            secs(13),
            Event::ConnectDone {
                ssid: "A".into(),
                result: Ok(()),
            },
        );
        assert_eq!(action, Action::None);
        assert!(matches!(engine.state(), State::Switching { .. }));
    }

    #[test]
    fn a_scan_that_never_answers_times_out_into_backoff() {
        let mut engine = engine_scanning();
        assert_eq!(engine.step(secs(65), connected("A", -80)), Action::None);
        assert!(matches!(engine.state(), State::Scanning { .. }));
        engine.step(secs(70), connected("A", -80));
        assert!(matches!(engine.state(), State::Backoff { .. }));
    }

    #[test]
    fn turning_auto_switch_off_pauses_and_on_resumes() {
        let mut engine = engine_scanning();
        engine.set_config(Config {
            auto_switch: false,
            ..config()
        });
        assert_eq!(engine.state(), &State::Paused(PauseReason::User));
        // The scan that was already running finishes, but nothing happens.
        assert_eq!(
            engine.step(secs(12), Event::ScanDone(Ok(vec![seen("B", -40)]))),
            Action::None
        );
        assert_eq!(
            engine.step(secs(15), Event::Sample(Sample::Disconnected)),
            Action::None
        );

        engine.set_config(config());
        assert_eq!(engine.state(), &WATCHING);
    }

    #[test]
    fn starts_paused_when_auto_switch_is_off() {
        let engine = Engine::new(Config {
            auto_switch: false,
            ..config()
        });
        assert_eq!(engine.state(), &State::Paused(PauseReason::User));
    }

    #[test]
    fn waking_up_waits_before_acting() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), Event::Sleep);
        assert_eq!(engine.state(), &State::Paused(PauseReason::Sleep));
        engine.step(secs(100), Event::Wake);
        assert_eq!(engine.state(), &State::Cooldown { until: secs(115) });
        assert_eq!(
            engine.step(secs(105), Event::Sample(Sample::Disconnected)),
            Action::None
        );
        assert_eq!(
            engine.step(secs(115), Event::Sample(Sample::Disconnected)),
            Action::Scan
        );
    }

    fn connect_done(ssid: &str, result: Result<(), Error>) -> Event {
        Event::ConnectDone {
            ssid: ssid.into(),
            result,
        }
    }

    #[test]
    fn user_switch_connects_and_cools_down() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), connected("A", -60));
        let action = engine.step(secs(1), Event::UserSwitch { ssid: "B".into() });
        assert_eq!(action, Action::Connect { ssid: "B".into() });
        let action = engine.step(secs(3), connect_done("B", Ok(())));
        assert_eq!(
            action,
            Action::Notify(Notice::Switched {
                from: Some("A".into()),
                to: "B".into(),
                reason: Reason::User,
            })
        );
        assert_eq!(engine.state(), &State::Cooldown { until: secs(33) });
    }

    #[test]
    fn user_switch_to_the_current_network_does_nothing() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), connected("A", -60));
        assert_eq!(
            engine.step(secs(1), Event::UserSwitch { ssid: "A".into() }),
            Action::None
        );
    }

    #[test]
    fn failed_user_switch_goes_back_to_watching_without_backoff() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), connected("A", -60));
        engine.step(secs(1), Event::UserSwitch { ssid: "B".into() });
        let action = engine.step(secs(2), connect_done("B", Err(Error::AuthFailed)));
        assert_eq!(
            action,
            Action::Notify(Notice::ConnectFailed {
                ssid: "B".into(),
                error: Error::AuthFailed,
            })
        );
        assert_eq!(engine.state(), &WATCHING);
        assert_eq!(engine.avoided().count(), 1);
    }

    #[test]
    fn user_switch_works_while_paused_and_stays_paused() {
        let paused = Config {
            auto_switch: false,
            ..config()
        };
        let mut engine = Engine::new(paused);
        engine.step(secs(0), connected("A", -60));
        assert_eq!(
            engine.step(secs(1), Event::UserSwitch { ssid: "B".into() }),
            Action::Connect { ssid: "B".into() }
        );
        engine.step(secs(2), connect_done("B", Ok(())));
        assert_eq!(engine.state(), &State::Paused(PauseReason::User));
    }

    #[test]
    fn user_switch_takes_over_from_a_scan() {
        let mut engine = engine_scanning();
        engine.step(secs(11), Event::UserSwitch { ssid: "B".into() });
        // The scan that was running finishes late: ignored.
        assert_eq!(
            engine.step(secs(12), Event::ScanDone(Ok(vec![seen("A", -40)]))),
            Action::None
        );
        assert!(matches!(
            engine.state(),
            State::Switching {
                reason: Reason::User,
                ..
            }
        ));
    }

    #[test]
    fn waking_up_does_not_undo_a_user_pause() {
        let mut engine = Engine::new(config());
        engine.step(secs(0), Event::Sleep);
        engine.set_config(Config {
            auto_switch: false,
            ..config()
        });
        engine.step(secs(100), Event::Wake);
        assert_eq!(engine.state(), &State::Paused(PauseReason::User));
    }
}
