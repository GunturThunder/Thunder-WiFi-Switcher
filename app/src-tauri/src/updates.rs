//! Updates: look for a new version on GitHub Releases now and then, download
//! it in the background, and install it only when the user chooses "Restart
//! to Update" (menu or About tab). Nothing restarts by itself.
//!
//! Each update file is signed with the Tauri updater key (CONTRIBUTING.md →
//! Releasing). The plugin refuses a file whose signature doesn't match the
//! public key in `tauri.conf.json`.
//!
//! macOS builds aren't signed by Apple, so after an update macOS forgets the
//! Location permission; the setup guide then opens by itself (see `lib.rs`).

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

use crate::status::Alert;
use crate::{AppState, notify};

/// The first check waits this long, so it doesn't slow down starting.
const FIRST_CHECK: Duration = Duration::from_secs(60);
/// Checks are this far apart (by the clock, so time asleep counts).
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);
/// How often the background thread looks at the clock.
const WAKE_EVERY: Duration = Duration::from_secs(30 * 60);

/// Where updating stands, for the window and the menu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// Not checked yet since the app started.
    #[default]
    Idle,
    Checking,
    /// This is the newest version.
    UpToDate,
    Downloading {
        version: String,
        /// How much has arrived, when the server says how big it is.
        percent: Option<u8>,
    },
    /// Downloaded. Installs when the user restarts the app from the menu or
    /// the About tab.
    Ready {
        version: String,
    },
    /// Checking, downloading or installing went wrong. Tries again later.
    Failed {
        message: String,
    },
}

/// Updating state, kept in `AppState`.
#[derive(Default)]
pub struct Updates {
    status: Mutex<UpdateStatus>,
    /// The downloaded update, waiting for the user.
    ready: Mutex<Option<(Update, Vec<u8>)>>,
}

impl Updates {
    pub fn status(&self) -> UpdateStatus {
        lock(&self.status).clone()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Checks in the background, first after a minute and then twice a day,
/// while "Check for updates automatically" is on.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = thread::Builder::new()
        .name("thunder-updates".into())
        .spawn(move || {
            thread::sleep(FIRST_CHECK);
            let mut last_check: Option<SystemTime> = None;
            loop {
                let due =
                    last_check.is_none_or(|at| at.elapsed().unwrap_or_default() >= CHECK_EVERY);
                if due && app.state::<AppState>().config().check_for_updates {
                    tauri::async_runtime::block_on(check(&app));
                    last_check = Some(SystemTime::now());
                }
                thread::sleep(WAKE_EVERY);
            }
        });
    if let Err(err) = spawned {
        log::error!("couldn't start checking for updates: {err}");
    }
}

/// Looks for a newer version and downloads it. Does nothing while an update
/// is already downloading or waiting to be installed.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let state = app.state::<AppState>();
    if matches!(
        state.updates.status(),
        UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Ready { .. }
    ) {
        return state.updates.status();
    }
    set(app, UpdateStatus::Checking);

    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(err) => Err(err),
    };
    match found {
        Ok(None) => set(app, UpdateStatus::UpToDate),
        Ok(Some(update)) => download(app, update).await,
        Err(err) => {
            log::warn!("couldn't check for updates: {err}");
            set(app, after_failed_check(&err));
        }
    }
    state.updates.status()
}

async fn download(app: &AppHandle, update: Update) {
    let version = update.version.clone();
    log::info!("downloading version {version}");
    set(
        app,
        UpdateStatus::Downloading {
            version: version.clone(),
            percent: None,
        },
    );

    let mut received: u64 = 0;
    let mut shown: Option<u8> = None;
    let downloaded = update
        .download(
            |chunk, total| {
                received = received.saturating_add(chunk as u64);
                let percent = percent(received, total);
                if percent != shown {
                    shown = percent;
                    set(
                        app,
                        UpdateStatus::Downloading {
                            version: version.clone(),
                            percent,
                        },
                    );
                }
            },
            || {},
        )
        .await;

    match downloaded {
        Ok(bytes) => {
            log::info!("version {version} is ready to install");
            *lock(&app.state::<AppState>().updates.ready) = Some((update, bytes));
            set(
                app,
                UpdateStatus::Ready {
                    version: version.clone(),
                },
            );
            if app.state::<AppState>().config().notify_on_switch {
                notify(
                    app,
                    &Alert {
                        title: format!("Version {version} is ready"),
                        body:
                            "Choose \"Restart to Update\" in the Thunder WiFi menu to install it."
                                .into(),
                    },
                );
            }
        }
        Err(err) => {
            log::warn!("couldn't download version {version}: {err}");
            set(
                app,
                UpdateStatus::Failed {
                    message: format!(
                        "Couldn't download version {version}. It will try again later."
                    ),
                },
            );
        }
    }
}

/// What a failed check means for the user. GitHub answering "not found" isn't
/// a problem: no version is published yet (or none for this computer).
fn after_failed_check(err: &Error) -> UpdateStatus {
    match err {
        Error::ReleaseNotFound | Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            UpdateStatus::UpToDate
        }
        Error::Reqwest(_) | Error::Network(_) => UpdateStatus::Failed {
            message: "Couldn't reach GitHub. Check your internet connection, or try again later."
                .into(),
        },
        _ => UpdateStatus::Failed {
            message: "Couldn't check for updates. Try again later.".into(),
        },
    }
}

/// How much of the download has arrived, when the server said how big it is.
fn percent(received: u64, total: Option<u64>) -> Option<u8> {
    let total = total.filter(|total| *total > 0)?;
    // In 128 bits, so even absurd sizes can't overflow.
    let percent = u128::from(received) * 100 / u128::from(total);
    Some(percent.min(100) as u8)
}

/// Installs the downloaded update and restarts into it. On Windows the
/// installer takes over: the app quits and the installer opens it again.
pub fn install_and_restart(app: &AppHandle) -> Result<(), String> {
    let ready = lock(&app.state::<AppState>().updates.ready).take();
    let Some((update, bytes)) = ready else {
        return Err("No update has been downloaded yet.".into());
    };
    log::info!("installing version {}", update.version);
    if let Err(err) = update.install(&bytes) {
        log::error!("couldn't install version {}: {err}", update.version);
        let message = format!(
            "Couldn't install version {}. Download it from GitHub instead.",
            update.version
        );
        set(
            app,
            UpdateStatus::Failed {
                message: message.clone(),
            },
        );
        return Err(message);
    }
    app.restart()
}

/// Records a new status and shows it in the menu and the window.
fn set(app: &AppHandle, status: UpdateStatus) {
    let state = app.state::<AppState>();
    {
        let mut current = lock(&state.updates.status);
        if *current == status {
            return;
        }
        current.clone_from(&status);
    }
    let ready = match &status {
        UpdateStatus::Ready { version } => Some(version.as_str()),
        _ => None,
    };
    if let Err(err) = state.tray.show_update(ready) {
        log::warn!("couldn't update the menu: {err}");
    }
    let _ = app.emit("update", &status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_published_is_not_an_error() {
        assert_eq!(
            after_failed_check(&Error::ReleaseNotFound),
            UpdateStatus::UpToDate
        );
        assert_eq!(
            after_failed_check(&Error::TargetNotFound("windows-aarch64".into())),
            UpdateStatus::UpToDate
        );
        let UpdateStatus::Failed { message } =
            after_failed_check(&Error::Network("offline".into()))
        else {
            panic!("a network problem is a failure");
        };
        assert!(message.contains("internet connection"), "{message}");
        let UpdateStatus::Failed { message } = after_failed_check(&Error::EmptyEndpoints) else {
            panic!("anything else is a failure");
        };
        assert!(!message.contains("internet"), "{message}");
    }

    #[test]
    fn download_percent() {
        assert_eq!(percent(0, Some(200)), Some(0));
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(200, Some(200)), Some(100));
        // The server's size was a little off: never more than 100.
        assert_eq!(percent(250, Some(200)), Some(100));
        // Size unknown, or nonsense.
        assert_eq!(percent(50, None), None);
        assert_eq!(percent(50, Some(0)), None);
        assert_eq!(percent(u64::MAX, Some(u64::MAX)), Some(100));
        assert_eq!(percent(u64::MAX / 2, Some(u64::MAX)), Some(49));
    }
}
