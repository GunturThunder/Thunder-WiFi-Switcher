//! Commands the settings window calls (`invoke` in `ui/src/lib/api.ts`).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;
use thunder_core::{Config, Error};

use crate::status::{JoinError, Status};
use crate::{AppState, apply_config};

type CommandResult<T> = Result<T, String>;

fn text(err: impl std::fmt::Display) -> String {
    err.to_string()
}

/// Facts about this copy of the app.
#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    platform: &'static str,
    simulated: bool,
    passwords_editable: bool,
    config_path: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS,
        simulated: state.simulated,
        passwords_editable: state.passwords.editable(),
        config_path: state.config_path.display().to_string(),
    }
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.worker.status()
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config().clone()
}

/// A note for each value that was adjusted while saving.
#[derive(Serialize)]
pub struct Saved {
    config: Config,
    notes: Vec<String>,
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: Config) -> CommandResult<Saved> {
    let (config, notes) = apply_config(&app, config)?;
    Ok(Saved { config, notes })
}

/// Joins a network ("Join now") and waits, so the window can say what went
/// wrong and how to fix it.
#[tauri::command]
pub async fn switch_to(app: AppHandle, ssid: String) -> Result<(), JoinError> {
    let wanted = ssid.clone();
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().worker.join(wanted))
        .await
        .unwrap_or_else(|err| Err(Error::Os(err.to_string())))
        .map_err(|err| JoinError::new(&ssid, &err))
}

/// A network in range: its strongest access point.
#[derive(Serialize)]
pub struct Nearby {
    ssid: String,
    rssi: i32,
}

/// Scans (on the worker thread) and lists networks in range, strongest first.
#[tauri::command]
pub async fn nearby_networks(app: AppHandle) -> CommandResult<Vec<Nearby>> {
    let scan = tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().worker.scan())
        .await
        .map_err(text)?
        .map_err(text)?;
    let mut strongest: HashMap<String, i32> = HashMap::new();
    for result in scan.into_iter().filter(|r| !r.ssid.is_empty()) {
        let rssi = strongest.entry(result.ssid).or_insert(result.rssi);
        *rssi = (*rssi).max(result.rssi);
    }
    let mut nearby: Vec<Nearby> = strongest
        .into_iter()
        .map(|(ssid, rssi)| Nearby { ssid, rssi })
        .collect();
    nearby.sort_by(|a, b| b.rssi.cmp(&a.rssi).then_with(|| a.ssid.cmp(&b.ssid)));
    Ok(nearby)
}

/// Networks the computer already knows how to join.
#[tauri::command]
pub async fn known_networks(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    state.backend.known_networks().map_err(text)
}

/// Whether each network has a saved password.
#[tauri::command]
pub async fn password_status(
    state: State<'_, AppState>,
    ssids: Vec<String>,
) -> CommandResult<BTreeMap<String, bool>> {
    ssids
        .into_iter()
        .map(|ssid| {
            let saved = state.passwords.contains(&ssid).map_err(text)?;
            Ok((ssid, saved))
        })
        .collect()
}

/// Saves a network's password (empty = open network). The password is never
/// logged or sent back to the window.
#[tauri::command]
pub async fn set_password(
    state: State<'_, AppState>,
    ssid: String,
    password: String,
) -> CommandResult<()> {
    state.passwords.set(&ssid, &password).map_err(text)?;
    log::info!("saved the password for {ssid}");
    Ok(())
}

#[tauri::command]
pub async fn delete_password(state: State<'_, AppState>, ssid: String) -> CommandResult<()> {
    state.passwords.delete(&ssid).map_err(text)?;
    log::info!("deleted the password for {ssid}");
    Ok(())
}

#[tauri::command]
pub fn request_permission(state: State<'_, AppState>) {
    state.backend.request_permission();
}

/// Links the window may open. Fixed here, so the window can't open anything else.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    Repo,
    Privacy,
    Issues,
    LocationSettings,
}

#[tauri::command]
pub fn open_link(app: AppHandle, link: Link) -> CommandResult<()> {
    const REPO: &str = "https://github.com/GunturThunder/Thunder-WiFi-Switcher";
    let url = match link {
        Link::Repo => REPO.to_owned(),
        Link::Privacy => format!("{REPO}/blob/main/PRIVACY.md"),
        Link::Issues => format!("{REPO}/issues/new/choose"),
        Link::LocationSettings if cfg!(windows) => "ms-settings:privacy-location".to_owned(),
        Link::LocationSettings => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices"
                .to_owned()
        }
    };
    app.opener().open_url(url, None::<&str>).map_err(text)
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> CommandResult<bool> {
    app.autolaunch().is_enabled().map_err(text)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CommandResult<()> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(text)?;
    } else {
        autolaunch.disable().map_err(text)?;
    }
    log::info!("open at login: {enabled}");
    Ok(())
}

/// A message about the settings file to show once (e.g. it was unreadable).
#[tauri::command]
pub fn startup_notice(state: State<'_, AppState>) -> Option<String> {
    state.startup_notice.clone()
}
