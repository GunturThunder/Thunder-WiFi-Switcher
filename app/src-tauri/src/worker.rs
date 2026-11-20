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
use thunder_core::{Config, Driver, Error, Event, Notice, Sample, ScanResult, WifiBackend};

use crate::status::{LastEvent, Status, notice_text};

/// Something for the worker to do.
pub enum Command {
    /// Use new settings.
    SetConfig(Config),
    /// The computer is going to sleep.
    Sleep,
    /// The computer woke up.
    Wake,
    /// The user picked a network in the menu.
    SwitchTo(String),
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
    /// status changes.
    pub fn spawn(
        config: Config,
        backend: Arc<dyn WifiBackend>,
        simulated: bool,
        on_status: impl Fn(&Status) + Send + 'static,
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
                        Ok(Command::SwitchTo(ssid)) => {
                            log::info!("user asked to switch to {ssid}");
                            driver.handle(now, Event::UserSwitch { ssid })
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
                                let sample = read_sample(driver.backend());
                                let notices = match &sample {
                                    Ok(sample) => driver.handle(now, Event::Sample(sample.clone())),
                                    Err(_) => Vec::new(),
                                };
                                reading = Some(sample);
                                notices
                            }
                        }
                    };

                    for notice in &notices {
                        log::info!("{}", notice_text(notice));
                        last_event = Some(LastEvent::now(notice));
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
                    );
                    let mut current = shared.lock().unwrap_or_else(PoisonError::into_inner);
                    if *current != next {
                        log_change(&current, &next);
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
}

/// Logs connection and activity changes (not every signal reading).
fn log_change(before: &Status, after: &Status) {
    if before.connection != after.connection || before.ssid != after.ssid {
        log::info!("{}", after.summary());
    }
    if before.activity != after.activity {
        log::info!("now {:?}", after.activity);
    }
    if before.permission != after.permission {
        log::info!("Location permission: {:?}", after.permission);
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
        let (tx, rx) = mpsc::channel();
        let backend: Arc<dyn WifiBackend> = wifi.clone();
        let worker = Worker::spawn(config, backend, true, move |status| {
            let _ = tx.send(status.clone());
        });
        (worker, rx)
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

        worker.send(Command::SwitchTo("Dorm-2.4".into()));
        let status = wait_for(&updates, |s| s.ssid.as_deref() == Some("Dorm-2.4"));
        assert_eq!(status.last_event.unwrap().text, "Switched to Dorm-2.4");
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
        worker.send(Command::SetConfig(Config {
            auto_switch: false,
            ..config(&["A"])
        }));
        wait_for(&updates, |s| s.activity == Activity::Paused);
        worker.send(Command::Sleep);
        // Still paused by the user while asleep.
        worker.send(Command::Wake);
        let status = wait_for(&updates, |s| s.connection == Connection::Disconnected);
        assert_eq!(status.activity, Activity::Paused);
    }
}
