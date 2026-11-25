//! The menu bar (macOS) / tray (Windows) icon and its menu.
//!
//! ```text
//! Restart to Update to 0.5.1          (only while an update is ready)
//! ─────────
//! Connected: Dorm-5G · -58 dBm        (status, not clickable)
//! 15:42 – Switched from Dorm-2.4 …    (last event, not clickable)
//! ─────────
//! ✓ Auto-switch
//! Switch to · Dorm ▸  Dorm-5G ✓ / Dorm-2.4 / …   (only the place you're in)
//! ─────────
//! Settings…
//! ─────────
//! Quit Thunder WiFi Switcher
//! ```

use std::sync::{Mutex, PoisonError};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Wry};
use thunder_core::Config;

use crate::status::Status;

/// Menu item ids handled in `lib.rs`.
pub const MENU_AUTO_SWITCH: &str = "auto_switch";
/// Opens the settings window.
pub const MENU_SETTINGS: &str = "settings";
/// Quits the app.
pub const MENU_QUIT: &str = "quit";
/// Prefix of the "Switch to" items; the rest is the network name.
pub const MENU_SWITCH_PREFIX: &str = "switch:";
/// Installs the downloaded update and restarts.
pub const MENU_UPDATE: &str = "update";

/// The tray icon plus the menu items that change.
pub struct Tray {
    icon: TrayIcon,
    menu: Menu<Wry>,
    update_item: MenuItem<Wry>,
    update_separator: PredefinedMenuItem<Wry>,
    /// The version the update item offers, while it's in the menu.
    update_shown: Mutex<Option<String>>,
    status_item: MenuItem<Wry>,
    event_item: MenuItem<Wry>,
    auto_switch: CheckMenuItem<Wry>,
    switch_to: Submenu<Wry>,
    shown: Mutex<Shown>,
}

/// What's on screen now, to skip updates that change nothing.
#[derive(Default, PartialEq)]
struct Shown {
    bars: Option<Option<u8>>,
    label: Option<Option<String>>,
    summary: String,
    event: String,
    switch_targets: Option<(Vec<String>, Option<String>)>,
    switch_label: String,
}

impl Tray {
    /// Builds the icon and menu. `on_menu` gets the id of each clicked item.
    pub fn build(
        app: &AppHandle,
        config: &Config,
        status: &Status,
        on_menu: impl Fn(&AppHandle, &str) + Send + Sync + 'static,
    ) -> tauri::Result<Self> {
        let status_item = MenuItem::with_id(app, "status", status.summary(), false, None::<&str>)?;
        let event_item =
            MenuItem::with_id(app, "last_event", "No switches yet", false, None::<&str>)?;
        let auto_switch = CheckMenuItem::with_id(
            app,
            MENU_AUTO_SWITCH,
            "Auto-switch",
            true,
            config.auto_switch,
            None::<&str>,
        )?;
        let switch_to = Submenu::with_id(app, "switch_to", "Switch to", true)?;
        let settings =
            MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?;
        let quit = MenuItem::with_id(
            app,
            MENU_QUIT,
            "Quit Thunder WiFi Switcher",
            true,
            Some("CmdOrCtrl+Q"),
        )?;
        let menu = Menu::with_items(
            app,
            &[
                &status_item,
                &event_item,
                &PredefinedMenuItem::separator(app)?,
                &auto_switch,
                &switch_to,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &PredefinedMenuItem::separator(app)?,
                &quit,
            ],
        )?;

        let icon = TrayIconBuilder::with_id("main")
            .icon(icon_for(status.bars()))
            .icon_as_template(cfg!(target_os = "macos"))
            .tooltip("Thunder WiFi Switcher")
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(move |app, event| on_menu(app, event.id().as_ref()))
            .build(app)?;

        let update_item =
            MenuItem::with_id(app, MENU_UPDATE, "Restart to Update", true, None::<&str>)?;
        let tray = Self {
            icon,
            menu,
            update_item,
            update_separator: PredefinedMenuItem::separator(app)?,
            update_shown: Mutex::new(None),
            status_item,
            event_item,
            auto_switch,
            switch_to,
            shown: Mutex::new(Shown::default()),
        };
        tray.update(app, config, status)?;
        Ok(tray)
    }

    /// Brings the icon and menu up to date. Cheap when nothing changed.
    pub fn update(&self, app: &AppHandle, config: &Config, status: &Status) -> tauri::Result<()> {
        let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);

        let bars = status.bars();
        if shown.bars != Some(bars) {
            self.icon.set_icon(Some(icon_for(bars)))?;
            self.icon.set_icon_as_template(cfg!(target_os = "macos"))?;
            shown.bars = Some(bars);
        }

        let label = status.label().filter(|_| config.show_signal_label);
        if shown.label.as_ref() != Some(&label) {
            // A title shows next to the icon on macOS; Windows has no room for one.
            #[cfg(target_os = "macos")]
            self.icon.set_title(label.as_deref())?;
            shown.label = Some(label);
        }

        let summary = status.summary();
        if shown.summary != summary {
            self.icon
                .set_tooltip(Some(format!("Thunder WiFi Switcher – {summary}")))?;
            self.status_item.set_text(&summary)?;
            shown.summary = summary;
        }

        let event = match &status.last_event {
            Some(event) => format!("{} – {}", event.time, event.text),
            None => "No switches yet".into(),
        };
        if shown.event != event {
            self.event_item.set_text(&event)?;
            shown.event = event;
        }

        if self.auto_switch.is_checked()? != config.auto_switch {
            self.auto_switch.set_checked(config.auto_switch)?;
        }

        let targets: Vec<String> = config
            .networks_here(status.ssid.as_deref())
            .map(|network| network.ssid.clone())
            .collect();
        let switch_label = match &status.place {
            Some(place) => format!("Switch to · {place}"),
            None => "Switch to".to_owned(),
        };
        if shown.switch_label != switch_label {
            self.switch_to.set_text(&switch_label)?;
            shown.switch_label = switch_label;
        }
        let switch_targets = Some((targets, status.ssid.clone()));
        if shown.switch_targets != switch_targets {
            let (targets, current) = switch_targets.clone().unwrap_or_default();
            self.rebuild_switch_to(app, &targets, current.as_deref())?;
            shown.switch_targets = switch_targets;
        }
        Ok(())
    }

    /// Shows "Restart to Update to <version>" at the top of the menu while an
    /// update is ready, and removes it otherwise.
    pub fn show_update(&self, version: Option<&str>) -> tauri::Result<()> {
        let mut shown = self
            .update_shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if shown.as_deref() == version {
            return Ok(());
        }
        match version {
            Some(version) => {
                self.update_item
                    .set_text(format!("Restart to Update to {version}"))?;
                if shown.is_none() {
                    self.menu.insert(&self.update_item, 0)?;
                    self.menu.insert(&self.update_separator, 1)?;
                }
            }
            None => {
                self.menu.remove(&self.update_item)?;
                self.menu.remove(&self.update_separator)?;
            }
        }
        *shown = version.map(str::to_owned);
        Ok(())
    }

    /// Forces the "Switch to" checkmarks to be redrawn (a click toggles them).
    pub fn forget_switch_to(&self) {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .switch_targets = None;
    }

    fn rebuild_switch_to(
        &self,
        app: &AppHandle,
        targets: &[String],
        current: Option<&str>,
    ) -> tauri::Result<()> {
        while self.switch_to.remove_at(0)?.is_some() {}
        if targets.is_empty() {
            let empty = MenuItem::new(app, "No networks added yet", false, None::<&str>)?;
            self.switch_to.append(&empty)?;
            return Ok(());
        }
        for ssid in targets {
            let item = CheckMenuItem::with_id(
                app,
                format!("{MENU_SWITCH_PREFIX}{ssid}"),
                ssid,
                true,
                current == Some(ssid.as_str()),
                None::<&str>,
            )?;
            self.switch_to.append(&item as &dyn IsMenuItem<Wry>)?;
        }
        Ok(())
    }
}

/// The menu bar icon for a signal level, or the crossed-out icon for `None`.
fn icon_for(bars: Option<u8>) -> Image<'static> {
    #[cfg(target_os = "macos")]
    {
        match bars {
            Some(3) => {
                tauri::include_image!("../../assets/icons/menubar/ThunderWiFi3Template@2x.png")
            }
            Some(2) => {
                tauri::include_image!("../../assets/icons/menubar/ThunderWiFi2Template@2x.png")
            }
            Some(1) => {
                tauri::include_image!("../../assets/icons/menubar/ThunderWiFi1Template@2x.png")
            }
            Some(_) => {
                tauri::include_image!("../../assets/icons/menubar/ThunderWiFi0Template@2x.png")
            }
            None => {
                tauri::include_image!("../../assets/icons/menubar/ThunderWiFiOffTemplate@2x.png")
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // A black template icon would vanish on a dark taskbar; Windows gets
        // light and dark variants later. Until then, the colour icon.
        let _ = bars;
        tauri::include_image!("icons/tray.png")
    }
}
