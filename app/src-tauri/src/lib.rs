//! Tauri shell for Thunder WiFi Switcher.
//!
//! The app lives in the menu bar (macOS) or system tray (Windows). A worker
//! thread runs the switching engine against the Wi-Fi backend
//! ([`worker::Worker`]); the tray and the settings window show its status and
//! change its settings. Switching logic lives in `thunder-core`.
//!
//! Command-line flags:
//! - `--settings` opens the settings window at launch.
//! - `--simulate` uses pretend networks instead of real Wi-Fi.

mod backend;
mod commands;
mod paths;
mod simulate;
mod status;
mod tray;
mod window;
mod worker;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use thunder_core::{Config, Engine, Origin, WifiBackend};

use crate::backend::Passwords;
use crate::status::Status;
use crate::tray::{MENU_AUTO_SWITCH, MENU_QUIT, MENU_SETTINGS, MENU_SWITCH_PREFIX, Tray};
use crate::worker::{Command, Worker};

/// Everything the commands and menu need, shared by the whole app.
pub struct AppState {
    config: Mutex<Config>,
    config_path: PathBuf,
    startup_notice: Option<String>,
    worker: Worker,
    backend: Arc<dyn WifiBackend>,
    passwords: Passwords,
    tray: Tray,
    simulated: bool,
}

impl AppState {
    fn config(&self) -> MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Redraws the tray from the latest status and settings.
    fn refresh_tray(&self, app: &AppHandle, status: &Status) {
        let config = self.config().clone();
        if let Err(err) = self.tray.update(app, &config, status) {
            log::warn!("couldn't update the menu bar icon: {err}");
        }
    }
}

/// Saves new settings and passes them to the worker, tray and window.
/// Returns the settings as saved, plus a note for each adjusted value.
pub fn apply_config(app: &AppHandle, mut config: Config) -> Result<(Config, Vec<String>), String> {
    let notes = config.normalize();
    let state = app.state::<AppState>();
    config
        .save(&state.config_path)
        .map_err(|err| format!("Couldn't save settings: {err}"))?;
    *state.config() = config.clone();
    state.worker.send(Command::SetConfig(config.clone()));
    state.refresh_tray(app, &state.worker.status());
    let _ = app.emit("config", &config);
    Ok((config, notes))
}

/// Starts the app. Called from `main.rs`.
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let open_settings_on_start = args.iter().any(|arg| arg == "--settings");
    let simulate = args.iter().any(|arg| arg == "--simulate");

    let mut log = tauri_plugin_log::Builder::new()
        .clear_targets()
        .level(log::LevelFilter::Info)
        .max_file_size(5_000_000)
        .rotation_strategy(RotationStrategy::KeepOne);
    log = match paths::log_dir() {
        Some(path) => log.target(Target::new(TargetKind::Folder {
            path,
            file_name: Some("thunderwifi".into()),
        })),
        None => log.target(Target::new(TargetKind::LogDir { file_name: None })),
    };
    if cfg!(debug_assertions) {
        log = log.target(Target::new(TargetKind::Stdout));
    }

    tauri::Builder::default()
        // Must come first: a second launch just opens the settings window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::open_settings(app);
        }))
        .plugin(log.build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_status,
            commands::get_config,
            commands::save_config,
            commands::switch_to,
            commands::nearby_networks,
            commands::known_networks,
            commands::password_status,
            commands::set_password,
            commands::delete_password,
            commands::request_permission,
            commands::open_link,
            commands::get_autostart,
            commands::set_autostart,
            commands::startup_notice,
        ])
        .setup(move |app| {
            // Menu bar only: no Dock icon. `LSUIElement` in Info.plist does the
            // same for bundled builds; this also covers `tauri dev`.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let config_path = paths::config_file(&handle, simulate)?;
            let (mut config, origin, startup_notice) = load_config(&config_path);
            log::info!(
                "Thunder WiFi Switcher {} starting{}",
                app.package_info().version,
                if simulate {
                    " (simulated networks)"
                } else {
                    ""
                }
            );

            let (backend, passwords) = if simulate {
                if origin == Origin::FirstRun {
                    config.networks = simulate::NETWORKS
                        .iter()
                        .map(|ssid| thunder_core::Network::new(*ssid))
                        .collect();
                }
                backend::simulated(simulate::start())
            } else {
                backend::real()
            };

            let first_status = Status::new(
                &Engine::new(config.clone()),
                None,
                backend.permission(),
                None,
                simulate,
            );
            let tray = Tray::build(&handle, &config, &first_status, on_menu)?;

            let status_handle = handle.clone();
            let worker = Worker::spawn(
                config.clone(),
                Arc::clone(&backend),
                simulate,
                move |status| {
                    if let Some(state) = status_handle.try_state::<AppState>() {
                        state.refresh_tray(&status_handle, status);
                    }
                    let _ = status_handle.emit("status", status);
                },
            );

            let show_settings =
                open_settings_on_start || origin == Origin::FirstRun || config.networks.is_empty();
            app.manage(AppState {
                config: Mutex::new(config),
                config_path,
                startup_notice,
                worker,
                backend,
                passwords,
                tray,
                simulated: simulate,
            });
            // The worker may have published before the state existed.
            let state = app.state::<AppState>();
            state.refresh_tray(&handle, &state.worker.status());

            #[cfg(any(target_os = "macos", windows))]
            watch_sleep(&handle);

            if show_settings {
                window::open_settings(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Thunder WiFi Switcher")
        .run(|_app, event| {
            // Keep running in the tray after the settings window closes. An
            // explicit `exit()` (the Quit menu item) passes a code and still quits.
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}

/// Loads the settings, falling back to defaults (with a note for the window)
/// if the file can't be used.
fn load_config(path: &std::path::Path) -> (Config, Origin, Option<String>) {
    match Config::load(path) {
        Ok(loaded) => {
            for note in &loaded.notes {
                log::warn!("settings: {note}");
            }
            let notice = match &loaded.origin {
                Origin::Recovered { backup, error } => {
                    log::warn!("settings file unreadable ({error}); using defaults");
                    Some(format!(
                        "Your settings file couldn't be read, so the defaults are in use. The old file was kept as {}.",
                        backup.display()
                    ))
                }
                Origin::File | Origin::FirstRun => None,
            };
            (loaded.config, loaded.origin, notice)
        }
        Err(err) => {
            log::error!("couldn't read settings: {err}");
            (
                Config::default(),
                Origin::FirstRun,
                Some(format!(
                    "Your settings couldn't be read ({err}), so the defaults are in use."
                )),
            )
        }
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        MENU_SETTINGS => window::open_settings(app),
        MENU_QUIT => app.exit(0),
        MENU_AUTO_SWITCH => {
            let mut config = state.config().clone();
            config.auto_switch = !config.auto_switch;
            log::info!(
                "auto-switch turned {}",
                if config.auto_switch { "on" } else { "off" }
            );
            if let Err(err) = apply_config(app, config) {
                log::error!("{err}");
            }
        }
        _ => {
            if let Some(ssid) = id.strip_prefix(MENU_SWITCH_PREFIX) {
                state.worker.send(Command::SwitchTo(ssid.to_owned()));
                // Clicking toggled the checkmark; redraw it from the real state.
                state.tray.forget_switch_to();
                state.refresh_tray(app, &state.worker.status());
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn watch_sleep(app: &AppHandle) {
    use std::cell::RefCell;
    use wifi_macos::{PowerEvent, SleepWatcher};

    thread_local! {
        // Lives on the main thread for the whole run.
        static WATCHER: RefCell<Option<SleepWatcher>> = const { RefCell::new(None) };
    }
    let handle = app.clone();
    let watcher = SleepWatcher::new(move |event| {
        if let Some(state) = handle.try_state::<AppState>() {
            state.worker.send(match event {
                PowerEvent::Sleep => Command::Sleep,
                PowerEvent::Wake => Command::Wake,
            });
        }
    });
    WATCHER.with(|slot| *slot.borrow_mut() = Some(watcher));
}

#[cfg(windows)]
fn watch_sleep(app: &AppHandle) {
    use std::sync::OnceLock;
    use wifi_windows::{PowerEvent, PowerWatcher};

    static WATCHER: OnceLock<PowerWatcher> = OnceLock::new();
    let handle = app.clone();
    let watcher = PowerWatcher::new(move |event| {
        if let Some(state) = handle.try_state::<AppState>() {
            state.worker.send(match event {
                PowerEvent::Sleep => Command::Sleep,
                PowerEvent::Wake => Command::Wake,
            });
        }
    });
    match watcher {
        Ok(watcher) => {
            let _ = WATCHER.set(watcher);
        }
        Err(err) => log::warn!("{err}; the app won't pause while the PC sleeps"),
    }
}
