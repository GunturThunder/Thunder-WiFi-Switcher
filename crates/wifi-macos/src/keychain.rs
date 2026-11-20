//! Wi-Fi passwords in the macOS Keychain.
//!
//! Each password is a generic-password item with service `ThunderWiFi` and the
//! network name as the account. Passwords are only read at the moment of
//! connecting and are never logged or written anywhere else.

use security_framework::base::Error as KeychainError;
use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
use security_framework::passwords;
use thunder_core::{Error, Result};

/// `errSecItemNotFound`: no such item.
const NOT_FOUND: i32 = -25300;

/// The app's Wi-Fi passwords in the Keychain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordStore {
    service: String,
}

impl Default for PasswordStore {
    fn default() -> Self {
        Self::new(Self::SERVICE)
    }
}

impl PasswordStore {
    /// Keychain service name the app uses.
    pub const SERVICE: &'static str = "ThunderWiFi";

    /// A store under a different service name (tests use their own).
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// The Keychain service name.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Saves (or replaces) the password for a network. An empty password
    /// means an open network, so any stored one is removed instead.
    pub fn set(&self, ssid: &str, password: &str) -> Result<()> {
        if password.is_empty() {
            return self.delete(ssid);
        }
        passwords::set_generic_password(&self.service, ssid, password.as_bytes())
            .map_err(keychain_error)
    }

    /// Removes the password for a network. Not having one is fine.
    pub fn delete(&self, ssid: &str) -> Result<()> {
        match passwords::delete_generic_password(&self.service, ssid) {
            Err(err) if err.code() != NOT_FOUND => Err(keychain_error(err)),
            _ => Ok(()),
        }
    }

    /// Whether a password is saved for this network. Only looks at the item's
    /// attributes: reading the password itself would make macOS ask the user
    /// for permission (once per item, after every rebuild of an ad-hoc-signed
    /// app) just to show "Password saved".
    pub fn contains(&self, ssid: &str) -> Result<bool> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(&self.service)
            .account(ssid)
            .load_attributes(true)
            .limit(1)
            .search();
        match found {
            Ok(items) => Ok(!items.is_empty()),
            Err(err) if err.code() == NOT_FOUND => Ok(false),
            Err(err) => Err(keychain_error(err)),
        }
    }

    /// The saved password, read only when connecting.
    pub(crate) fn get(&self, ssid: &str) -> Result<Option<String>> {
        match passwords::get_generic_password(&self.service, ssid) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| Error::Os("Keychain: the saved password isn't valid text".into())),
            Err(err) if err.code() == NOT_FOUND => Ok(None),
            Err(err) => Err(keychain_error(err)),
        }
    }

    /// Networks that have a saved password, in no particular order.
    pub fn saved_networks(&self) -> Result<Vec<String>> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(&self.service)
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        match found {
            Ok(items) => Ok(items
                .iter()
                .filter_map(|item| item.simplify_dict()?.remove("acct"))
                .collect()),
            Err(err) if err.code() == NOT_FOUND => Ok(Vec::new()),
            Err(err) => Err(keychain_error(err)),
        }
    }
}

fn keychain_error(err: KeychainError) -> Error {
    Error::Os(format!("Keychain: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes to the real login Keychain, under a test-only service name.
    /// Run with `cargo test -p wifi-macos -- --ignored`.
    #[test]
    #[ignore = "writes to the login Keychain"]
    fn save_read_list_and_delete() {
        let store = PasswordStore::new("ThunderWiFi-test");
        let ssid = format!("test-network-{}", std::process::id());

        store.set(&ssid, "hunter22").unwrap();
        assert_eq!(store.get(&ssid).unwrap().as_deref(), Some("hunter22"));
        assert!(store.saved_networks().unwrap().contains(&ssid));

        store.set(&ssid, "changed").unwrap();
        assert_eq!(store.get(&ssid).unwrap().as_deref(), Some("changed"));

        store.set(&ssid, "").unwrap();
        assert_eq!(store.get(&ssid).unwrap(), None);
        assert!(!store.contains(&ssid).unwrap());
        store.delete(&ssid).unwrap();
    }

    #[test]
    fn default_service_is_thunderwifi() {
        assert_eq!(PasswordStore::default().service(), "ThunderWiFi");
    }
}
