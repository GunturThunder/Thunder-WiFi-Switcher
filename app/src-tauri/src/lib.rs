//! Tauri shell for Thunder WiFi Switcher.
//!
//! The app lives in the menu bar (macOS) or system tray (Windows). It has no
//! main window: the settings window is created on demand and destroyed when
//! closed, to keep memory low. Switching logic lives in `thunder-core`; this
//! crate only wires it to the tray, the settings window and the OS backend.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

const SETTINGS_WINDOW: &str = "settings";
const MENU_SETTINGS: &str = "settings";
const MENU_QUIT: &str = "quit";

/// Starts the app. Called from `main.rs`.
pub fn run() {
    let open_settings_on_start = std::env::args().any(|arg| arg == "--settings");

    tauri::Builder::default()
        .setup(move |app| {
            // Menu bar only: no Dock icon. `LSUIElement` in Info.plist does the
            // same for bundled builds; this also covers `tauri dev`.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            build_tray(app.handle())?;

            if open_settings_on_start {
                open_settings(app.handle());
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

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Not running yet", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(
        app,
        MENU_QUIT,
        "Quit Thunder WiFi Switcher",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(tray_icon())
        // macOS tints template images to match a light or dark menu bar.
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Thunder WiFi Switcher")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SETTINGS => open_settings(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Icon shown in the menu bar or tray.
///
/// macOS uses the black template icons from `assets/icons/menubar` (18 pt, drawn
/// at @2x). Phase 3 switches between the 0–3 arc and "Off" variants to show
/// signal level; until then it shows full signal.
///
/// Windows keeps the colour icon for now: a black icon would disappear on a
/// dark taskbar, so it needs light and dark variants first.
fn tray_icon() -> Image<'static> {
    #[cfg(target_os = "macos")]
    {
        tauri::include_image!("../../assets/icons/menubar/ThunderWiFi3Template@2x.png")
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri::include_image!("icons/tray.png")
    }
}

/// Shows the settings window, creating it if it isn't open.
fn open_settings(app: &AppHandle) {
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
        Err(err) => eprintln!("could not open the settings window: {err}"),
    }
}
