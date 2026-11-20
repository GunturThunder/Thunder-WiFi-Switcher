//! Picks the Wi-Fi backend and password store for this platform, or the
//! pretend ones for `--simulate`.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use thunder_core::fake::FakeBackend;
use thunder_core::{Error, WifiBackend};

/// Where network passwords are kept.
pub enum Passwords {
    /// macOS Keychain (service `ThunderWiFi`).
    #[cfg(target_os = "macos")]
    Keychain(wifi_macos::PasswordStore),
    /// In memory, for `--simulate`: the fake backend doesn't need them.
    Memory(Mutex<HashSet<String>>),
    /// The OS keeps them (Windows uses its own saved profiles).
    #[cfg(not(target_os = "macos"))]
    Os,
}

impl Passwords {
    /// Whether the settings window should show password fields.
    pub fn editable(&self) -> bool {
        match self {
            #[cfg(not(target_os = "macos"))]
            Self::Os => false,
            _ => true,
        }
    }

    /// Whether a password is saved for `ssid`.
    pub fn contains(&self, ssid: &str) -> Result<bool, Error> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Keychain(store) => store.contains(ssid),
            Self::Memory(saved) => Ok(lock(saved).contains(ssid)),
            #[cfg(not(target_os = "macos"))]
            Self::Os => Ok(false),
        }
    }

    /// Saves a password; empty means an open network.
    pub fn set(&self, ssid: &str, password: &str) -> Result<(), Error> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Keychain(store) => store.set(ssid, password),
            Self::Memory(saved) => {
                if password.is_empty() {
                    lock(saved).remove(ssid);
                } else {
                    lock(saved).insert(ssid.to_owned());
                }
                Ok(())
            }
            #[cfg(not(target_os = "macos"))]
            Self::Os => Err(Error::Os(
                "passwords are managed by the operating system".into(),
            )),
        }
    }

    /// Removes a saved password.
    pub fn delete(&self, ssid: &str) -> Result<(), Error> {
        self.set(ssid, "")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The backend for real Wi-Fi on this platform.
pub fn real() -> (Arc<dyn WifiBackend>, Passwords) {
    #[cfg(target_os = "macos")]
    {
        let backend = wifi_macos::MacBackend::new();
        let passwords = Passwords::Keychain(backend.passwords().clone());
        (Arc::new(backend), passwords)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (Arc::new(Unsupported), Passwords::Os)
    }
}

/// The pretend backend for `--simulate`.
pub fn simulated(fake: Arc<FakeBackend>) -> (Arc<dyn WifiBackend>, Passwords) {
    (fake, Passwords::Memory(Mutex::new(HashSet::new())))
}

/// Stand-in until the Windows backend exists: reports that Wi-Fi can't be
/// read, so the app runs but never switches.
#[cfg(not(target_os = "macos"))]
struct Unsupported;

#[cfg(not(target_os = "macos"))]
impl WifiBackend for Unsupported {
    fn current(&self) -> thunder_core::Result<Option<thunder_core::Link>> {
        Err(unsupported())
    }
    fn scan(&self) -> thunder_core::Result<Vec<thunder_core::ScanResult>> {
        Err(unsupported())
    }
    fn connect(&self, _ssid: &str) -> thunder_core::Result<()> {
        Err(unsupported())
    }
    fn known_networks(&self) -> thunder_core::Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn gateway(&self) -> thunder_core::Result<Option<std::net::IpAddr>> {
        Ok(None)
    }
    fn permission(&self) -> thunder_core::Permission {
        thunder_core::Permission::Granted
    }
    fn request_permission(&self) {}
    fn powered(&self) -> bool {
        true
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported() -> Error {
    Error::Os("Windows support is coming in v0.5 (try --simulate)".into())
}
