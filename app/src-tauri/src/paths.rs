//! Where the settings file and logs live (see docs/TECHNICAL-DESIGN.md).
//!
//! | OS | Settings | Logs |
//! | --- | --- | --- |
//! | macOS | `~/Library/Application Support/ThunderWiFi/config.toml` | `~/Library/Logs/ThunderWiFi/` |
//! | Windows | `%APPDATA%\ThunderWiFi\config.toml` | `%LOCALAPPDATA%\ThunderWiFi\logs\` |

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

const FOLDER: &str = "ThunderWiFi";

/// The settings file. `--simulate` uses its own, so pretend networks never
/// end up in the real settings.
pub fn config_file(app: &AppHandle, simulate: bool) -> tauri::Result<PathBuf> {
    let name = if simulate {
        "config-simulate.toml"
    } else {
        "config.toml"
    };
    Ok(app.path().config_dir()?.join(FOLDER).join(name))
}

/// The log folder, or `None` to use the platform default.
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs").join(FOLDER))
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join(FOLDER).join("logs"))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}
