//! Runs the [`Engine`] against a [`WifiBackend`]: reads a sample, checks the
//! internet quality, does what the engine asks (scan, connect), feeds the
//! results back, and repeats until there's nothing left to do.
//!
//! The app shell calls [`Driver::tick`] every [`crate::rules::SAMPLE_INTERVAL`]
//! on a worker thread, since scans, connects and probes block. Tests use the
//! same driver with [`crate::fake::FakeBackend`].

use std::time::Duration;

use crate::config::Config;
use crate::engine::{Action, Engine, Event, Notice, Sample, State};
use crate::quality::{Internet, Prober, Quality, verdict};
use crate::wifi::{Result, WifiBackend};

/// The engine plus the backend it controls.
#[derive(Debug)]
pub struct Driver<B> {
    engine: Engine,
    backend: B,
    prober: Prober,
    quality: Option<Quality>,
}

impl<B: WifiBackend> Driver<B> {
    /// A driver with a fresh engine.
    pub fn new(config: Config, backend: B) -> Self {
        Self {
            engine: Engine::new(config),
            backend,
            prober: Prober::new(),
            quality: None,
        }
    }

    /// The engine, for reading its state.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The backend, for reading signal or permission directly.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The latest internet-quality results, while connected and not paused.
    pub fn quality(&self) -> Option<&Quality> {
        self.quality.as_ref()
    }

    /// Applies new settings.
    pub fn set_config(&mut self, config: Config) {
        self.engine.set_config(config);
    }

    /// Reads one sample, checks the internet behind it, and acts on it.
    /// Returns what the user should be told.
    ///
    /// Fails only if the current link can't be read (for example, Location
    /// permission is missing); the engine doesn't see a sample then.
    pub fn tick(&mut self, now: Duration) -> Result<Vec<Notice>> {
        let mut sample = read_sample(&self.backend)?;
        let paused = matches!(self.engine.state(), State::Paused(_));
        match &mut sample {
            // Checks stop while paused (and, via the worker, while asleep).
            Sample::Connected(link, internet) if !paused => {
                let config = self.engine.config();
                let router = self.backend.gateway().ok().flatten();
                let targets = config.ping_targets.clone();
                let (bad_ping, bad_loss) = (config.bad_ping_ms, config.bad_loss_pct);
                let quality = self
                    .prober
                    .probe(&self.backend, &link.ssid, router, &targets);
                *internet = verdict(&quality, bad_ping, bad_loss);
                self.quality = Some(quality);
            }
            _ => {
                self.prober.reset();
                self.quality = None;
            }
        }
        Ok(self.handle(now, Event::Sample(sample)))
    }

    /// Feeds any event (sleep, wake, a sample) and carries out the resulting
    /// actions. Scans and connects run here, so this blocks while they do.
    ///
    /// Every step uses the same `now`: a scan takes a few seconds at most,
    /// which is small next to the engine's 30-second timers.
    pub fn handle(&mut self, now: Duration, event: Event) -> Vec<Notice> {
        let mut notices = Vec::new();
        let mut action = self.engine.step(now, event);
        loop {
            action = match action {
                Action::None => break,
                Action::Notify(notice) => {
                    notices.push(notice);
                    break;
                }
                Action::Scan => {
                    let result = self.backend.scan();
                    self.engine.step(now, Event::ScanDone(result))
                }
                Action::Connect { ssid } => {
                    let result = self.backend.connect(&ssid);
                    self.engine.step(now, Event::ConnectDone { ssid, result })
                }
            };
        }
        notices
    }
}

/// Turns what the backend reports into a [`Sample`]. The internet verdict is
/// left [`Internet::Unknown`]; [`Driver::tick`] fills it in.
pub fn read_sample(backend: &impl WifiBackend) -> Result<Sample> {
    if !backend.powered() {
        return Ok(Sample::WifiOff);
    }
    Ok(match backend.current()? {
        Some(link) => Sample::Connected(link, Internet::Unknown),
        None => Sample::Disconnected,
    })
}
