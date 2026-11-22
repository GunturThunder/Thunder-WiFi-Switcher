//! The worker thread: runs the engine against the Wi-Fi backend every 5
//! seconds, and carries out what the menu and settings window ask for.
//!
//! Scans and connects block for seconds, so they never run on the main thread.
//! Everything that touches the backend's radio goes through this one thread,
//! so two scans never overlap.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use thunder_core::driver::read_sample;
use thunder_core::rules::SAMPLE_INTERVAL;
use thunder_core::{
    Config, Driver, Error, Event, Notice, Quality, Sample, ScanResult, WifiBackend,
};

use crate::status::{Alert, InternetState, LastEvent, Status, notice_text};

/// Something for the worker to do.
pub enum Command {
    /// Use new settings.
    SetConfig(Config),
    /// The computer is going to sleep (sent by the macOS and Windows power
    /// watchers).
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Sleep,
    /// The computer woke up.
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Wake,
    /// The user picked a network: in the menu (no reply), or with "Join now"
    /// in the settings window, which waits for the result.
    SwitchTo {
        ssid: String,
        reply: Option<Sender<Result<(), Error>>>,
    },
    /// Scan now and send the results back (settings window).
    Scan(Sender<Result<Vec<ScanResult>, Error>>),
}

/// Handle to the worker thread. Dropping it stops the thread.
pub struct Worker {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
}

impl Worker {
    /// Starts the worker. `on_status` runs on the worker thread whenever the
    /// status changes; `on_alert`, for each notification to show (only when
    /// the user turned them on).
    pub fn spawn(
        config: Config,
        backend: Arc<dyn WifiBackend>,
        simulated: bool,
        on_status: impl Fn(&Status) + Send + 'static,
        on_alert: impl Fn(&Alert) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut driver = Driver::new(config, backend);
        let status = Arc::new(Mutex::new(Status::new(
            driver.engine(),
            None,
            driver.backend().permission(),
            None,
            simulated,
        )));
        let shared = Arc::clone(&status);

        thread::Builder::new()
            .name("thunder-worker".into())
            .spawn(move || {
                let started = Instant::now();
                let mut reading: Option<Result<Sample, Error>> = None;
                let mut last_event: Option<LastEvent> = None;
                let mut asleep = false;
                let mut next_tick = Instant::now();

                loop {
                    let wait = next_tick.saturating_duration_since(Instant::now());
                    let command = rx.recv_timeout(wait);
                    let now = started.elapsed();
                    let notices: Vec<Notice> = match command {
                        Ok(Command::SetConfig(config)) => {
                            driver.set_config(config);
                            Vec::new()
                        }
                        Ok(Command::Sleep) => {
                            log::info!("going to sleep: pausing");
                            asleep = true;
                            driver.handle(now, Event::Sleep)
                        }
                        Ok(Command::Wake) => {
                            log::info!("woke up");
                            asleep = false;
                            next_tick = Instant::now();
                            driver.handle(now, Event::Wake)
                        }
                        Ok(Command::SwitchTo { ssid, reply }) => {
                            log::info!("user asked to switch to {ssid}");
                            let notices =
                                driver.handle(now, Event::UserSwitch { ssid: ssid.clone() });
                            if let Some(reply) = reply {
                                let _ = reply.send(join_result(&ssid, &notices));
                            }
                            notices
                        }
                        Ok(Command::Scan(reply)) => {
                            let _ = reply.send(driver.backend().scan());
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {
                            next_tick += SAMPLE_INTERVAL;
                            if next_tick <= Instant::now() {
                                // Fell behind (e.g. a long scan): don't catch up in a burst.
                                next_tick = Instant::now() + SAMPLE_INTERVAL;
                            }
                            if asleep {
                                Vec::new()
                            } else {
                                // Reads the link, checks the internet, acts.
                                match driver.tick(now) {
                                    Ok(notices) => {
                                        reading = driver.engine().last_sample().cloned().map(Ok);
                                        notices
                                    }
                                    Err(err) => {
                                        reading = Some(Err(err));
                                        Vec::new()
                                    }
                                }
                            }
                        }
                    };

                    let notify = driver.engine().config().notify_on_switch;
                    for notice in &notices {
                        log::info!("{}", notice_text(notice));
                        last_event = Some(LastEvent::now(notice));
                        if let Some(alert) = Alert::for_notice(notice).filter(|_| notify) {
                            on_alert(&alert);
                        }
                    }
                    if notices
                        .iter()
                        .any(|notice| matches!(notice, Notice::Switched { .. }))
                    {
                        // Show the new network straight away, not in 5 s.
                        reading = Some(read_sample(driver.backend()));
                    }

                    let next = Status::new(
                        driver.engine(),
                        reading.as_ref(),
                        driver.backend().permission(),
                        last_event.clone(),
                        simulated,
                    )
                    .with_quality(
                        driver.quality().and_then(|quality| quality.ping_ms()),
                        driver.quality().and_then(|quality| quality.loss_pct()),
                    );
                    let mut current = shared.lock().unwrap_or_else(PoisonError::into_inner);
                    if *current != next {
                        log_change(&current, &next, driver.quality());
                        *current = next.clone();
                        drop(current);
                        on_status(&next);
                    }
                }
            })
            .expect("can start the worker thread");

        Self {
            commands: tx,
            status,
        }
    }

    /// Sends a command. Does nothing if the worker has stopped.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// The latest status.
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Scans for networks on the worker thread and waits for the result.
    pub fn scan(&self) -> Result<Vec<ScanResult>, Error> {
        let (tx, rx) = mpsc::channel();
        self.send(Command::Scan(tx));
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| Error::Timeout)?
    }

    /// Joins a network on the worker thread and waits for the result.
    pub fn join(&self, ssid: String) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();
        self.send(Command::SwitchTo {
            ssid,
            reply: Some(tx),
        });
        rx.recv_timeout(JOIN_WAIT).map_err(|_| Error::Timeout)?
    }
}

/// How long "Join now" waits. Joining takes up to about 20 seconds, and macOS
/// may first ask the user to allow reading the password from the Keychain.
const JOIN_WAIT: Duration = Duration::from_secs(90);

/// Whether joining `ssid` worked, from what the engine reported. Nothing to
/// report means it was already the current network.
fn join_result(ssid: &str, notices: &[Notice]) -> Result<(), Error> {
    notices
        .iter()
        .find_map(|notice| match notice {
            Notice::ConnectFailed {
                ssid: failed,
                error,
            } if failed == ssid => Some(Err(error.clone())),
            _ => None,
        })
        .unwrap_or(Ok(()))
}

/// Logs connection and activity changes (not every signal reading). When the
/// internet turns bad, also logs what each server did, for network reports.
fn log_change(before: &Status, after: &Status, quality: Option<&Quality>) {
    if before.connection != after.connection || before.ssid != after.ssid {
        log::info!("{}", after.summary());
    }
    if before.activity != after.activity {
        log::info!("now {:?}", after.activity);
    }
    if before.permission != after.permission {
        log::info!("Location permission: {:?}", after.permission);
    }
    if before.internet != after.internet {
        match (after.internet, quality) {
            (InternetState::Bad | InternetState::BadLink, Some(quality)) => {
                log::info!("internet: {:?} ({})", after.internet, quality.describe());
            }
            _ => log::info!("internet: {:?}", after.internet),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    use thunder_core::fake::FakeBackend;
    use thunder_core::{Network, Permission};

    use crate::status::{Activity, Connection};

    fn wait_for(updates: &Receiver<Status>, what: impl Fn(&Status) -> bool) -> Status {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Ok(status) = updates.recv_timeout(Duration::from_millis(100))
                && what(&status)
            {
                return status;
            }
        }
        panic!("the expected status never came");
    }

    fn start(wifi: &Arc<FakeBackend>, config: Config) -> (Worker, Receiver<Status>) {
        let (worker, updates, _alerts) = start_with_alerts(wifi, config);
        (worker, updates)
    }

    fn start_with_alerts(
        wifi: &Arc<FakeBackend>,
        config: Config,
    ) -> (Worker, Receiver<Status>, Receiver<Alert>) {
        let (status_tx, status_rx) = mpsc::channel();
        let (alert_tx, alert_rx) = mpsc::channel();
        let backend: Arc<dyn WifiBackend> = wifi.clone();
        let worker = Worker::spawn(
            config,
            backend,
            true,
            move |status| {
                let _ = status_tx.send(status.clone());
            },
            move |alert| {
                let _ = alert_tx.send(alert.clone());
            },
        );
        (worker, status_rx, alert_rx)
    }

    fn config(ssids: &[&str]) -> Config {
        Config {
            networks: ssids.iter().map(|s| Network::new(*s)).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn reports_the_current_network() {
        let wifi = Arc::new(FakeBackend::new());
        wifi.set_network("Dorm-5G", -58);
        wifi.join("Dorm-5G");
        let (worker, updates) = start(&wifi, config(&["Dorm-5G"]));

        let status = wait_for(&updates, |s| s.connection == Connection::Connected);
        assert_eq!(status.ssid.as_deref(), Some("Dorm-5G"));
        assert_eq!(status.rssi, Some(-58));
        assert!(status.managed);
        assert_eq!(status.permission, Permission::Granted.into());
        assert_eq!(worker.status(), status);
    }

    #[test]
    fn reconnects_and_records_the_last_event() {
        let wifi = Arc::new(FakeBackend::new());
        wifi.set_network("Dorm-2.4", -65);
        let (_worker, updates) = start(&wifi, config(&["Dorm-5G", "Dorm-2.4"]));

        let status = wait_for(&updates, |s| s.last_event.is_some());
        assert_eq!(status.last_event.unwrap().text, "Reconnected to Dorm-2.4");
        assert_eq!(status.ssid.as_deref(), Some("Dorm-2.4"));
        assert_eq!(status.activity, Activity::Settling);
    }

    #[test]
    fn switches_when_the_user_asks() {
        let wifi = Arc::new(FakeBackend::new());
        wifi.set_network("Dorm-5G", -58);
        wifi.set_network("Dorm-2.4", -70);
        wifi.join("Dorm-5G");
        let (worker, updates) = start(&wifi, config(&["Dorm-5G", "Dorm-2.4"]));
        wait_for(&updates, |s| s.connection == Connection::Connected);

        worker.send(Command::SwitchTo {
            ssid: "Dorm-2.4".into(),
            reply: None,
        });
        let status = wait_for(&updates, |s| s.ssid.as_deref() == Some("Dorm-2.4"));
        assert_eq!(status.last_event.unwrap().text, "Switched to Dorm-2.4");
    }

    #[test]
    fn notifies_about_automatic_switches_only_when_turned_on() {
        let start_notifying = |on: bool| {
            let wifi = Arc::new(FakeBackend::new());
            wifi.set_network("Dorm-2.4", -65);
            wifi.set_network("Dorm-5G", -75);
            let config = Config {
                notify_on_switch: on,
                ..config(&["Dorm-5G", "Dorm-2.4"])
            };
            let (worker, updates, alerts) = start_with_alerts(&wifi, config);
            // Not connected at first, so the app reconnects by itself.
            wait_for(&updates, |s| s.last_event.is_some());
            (worker, updates, alerts)
        };

        let (worker, updates, alerts) = start_notifying(true);
        assert_eq!(alerts.try_recv().unwrap().title, "Reconnected to Dorm-2.4");
        // A switch the user asked for: they know, so no notification.
        worker.join("Dorm-5G".into()).unwrap();
        wait_for(&updates, |s| s.ssid.as_deref() == Some("Dorm-5G"));
        assert!(alerts.try_recv().is_err());

        // Turned off: nothing, even for an automatic reconnect.
        let (_worker, _updates, alerts) = start_notifying(false);
        assert!(alerts.try_recv().is_err());
    }

    #[test]
    fn join_reports_whether_it_worked() {
        let wifi = Arc::new(FakeBackend::new());
        wifi.set_network("Dorm-5G", -58);
        wifi.set_network("Dorm-2.4", -70);
        wifi.join("Dorm-5G");
        wifi.fail_connect("Dorm-2.4", Error::NoPassword);
        let (worker, updates) = start(&wifi, config(&["Dorm-5G", "Dorm-2.4", "Library"]));
        wait_for(&updates, |s| s.connection == Connection::Connected);

        assert_eq!(worker.join("Dorm-2.4".into()), Err(Error::NoPassword));
        assert_eq!(worker.join("Library".into()), Err(Error::NotFound));
        // Already there: nothing to do, and nothing went wrong.
        assert_eq!(worker.join("Dorm-5G".into()), Ok(()));

        wifi.set_powered(false);
        wait_for(&updates, |s| s.connection == Connection::Off);
        assert_eq!(worker.join("Dorm-2.4".into()), Err(Error::WifiOff));
    }

    #[test]
    fn scans_on_request() {
        let wifi = Arc::new(FakeBackend::new());
        wifi.set_network("Dorm-5G", -58);
        let (worker, _updates) = start(&wifi, config(&[]));
        let scan = worker.scan().unwrap();
        assert_eq!(scan.len(), 1);
        assert_eq!(scan[0].ssid, "Dorm-5G");
    }

    #[test]
    fn pausing_shows_in_the_status() {
        let wifi = Arc::new(FakeBackend::new());
        let (worker, updates) = start(&wifi, config(&["A"]));
        wait_for(&updates, |s| s.connection == Connection::Disconnected);
        worker.send(Command::SetConfig(Config {
            auto_switch: false,
            ..config(&["A"])
        }));
        wait_for(&updates, |s| s.activity == Activity::Paused);

        // Still paused by the user after sleeping. A user pause shows the
        // same through sleep, so no new status comes: ask for one instead,
        // once the worker has handled both (commands run in order).
        worker.send(Command::Sleep);
        worker.send(Command::Wake);
        worker.scan().unwrap();
        let status = worker.status();
        assert_eq!(status.activity, Activity::Paused);
        assert_eq!(status.connection, Connection::Disconnected);
    }
}
