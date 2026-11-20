//! The settings window: created on demand and destroyed when closed, to keep
//! memory low (the app spends most of its life with no window at all).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const SETTINGS_WINDOW: &str = "settings";

/// Shows the settings window, creating it if it isn't open.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let built = WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::default())
        .title("Thunder WiFi Switcher Settings")
        .inner_size(480.0, 560.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .build();

    match built {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(err) => log::error!("could not open the settings window: {err}"),
    }
}
