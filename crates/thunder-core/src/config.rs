//! Settings, stored in one human-readable TOML file. Passwords never go in it.
//!
//! People edit this file by hand, so loading is forgiving: one bad value is
//! replaced by its default (with a note saying so) instead of throwing the
//! whole file away. Only a file that isn't valid TOML at all is set aside as
//! `config.toml.bak` and replaced with defaults.
//!
//! The file's location is up to the app shell (`~/Library/Application
//! Support/ThunderWiFi/` on macOS, `%APPDATA%\ThunderWiFi\` on Windows), so
//! this module only takes paths.

use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use toml::Table;

/// Format version written to the file. Bump it, and add a step to the
/// private `migrate` function, whenever an existing setting changes meaning
/// or name.
pub const CURRENT_VERSION: u32 = 1;
/// Allowed weak-signal levels (dBm), matching the slider in Settings.
pub const WEAK_SIGNAL_RANGE: RangeInclusive<i32> = -80..=-65;
/// Allowed "bad ping" limits (ms).
pub const BAD_PING_RANGE: RangeInclusive<u32> = 50..=2000;
/// Allowed "bad packet loss" limits (%).
pub const BAD_LOSS_RANGE: RangeInclusive<u32> = 1..=100;

const HEADER: &str = "\
# Thunder WiFi Switcher settings.
# Networks are listed in priority order: higher in the list wins a tie.
# Wi-Fi passwords are never stored in this file.

";

/// All user settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    /// File format version; see [`CURRENT_VERSION`].
    pub version: u32,
    /// Whether the app switches networks by itself.
    pub auto_switch: bool,
    /// Signal below this (dBm) counts as weak.
    pub weak_signal_dbm: i32,
    /// Ping above this (ms) to both public targets counts as bad internet.
    pub bad_ping_ms: u32,
    /// Packet loss above this (%) to both public targets counts as bad internet.
    pub bad_loss_pct: u32,
    /// Show a notification after each switch.
    pub notify_on_switch: bool,
    /// Interface language.
    pub language: Language,
    /// Public servers used to check the internet connection.
    pub ping_targets: Vec<IpAddr>,
    /// The user's networks, in priority order.
    pub networks: Vec<Network>,
}

/// One of the user's networks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    /// Network name.
    pub ssid: String,
    /// Whether the app may switch to it.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl Network {
    /// An enabled network.
    pub fn new(ssid: impl Into<String>) -> Self {
        Self {
            ssid: ssid.into(),
            enabled: true,
        }
    }
}

/// Interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// English.
    #[default]
    En,
    /// Bahasa Indonesia.
    Id,
}

/// Where the loaded settings came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Read from the file.
    File,
    /// No file yet: these are the defaults.
    FirstRun,
    /// The file wasn't valid TOML. It was renamed to `backup` and defaults are
    /// used; the app should tell the user.
    Recovered {
        /// Where the unreadable file now is.
        backup: PathBuf,
        /// What was wrong with it.
        error: String,
    },
}

/// Result of [`Config::load`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The settings to use.
    pub config: Config,
    /// Where they came from.
    pub origin: Origin,
    /// Values that were ignored or adjusted, in plain words, for the log.
    pub notes: Vec<String>,
}

impl Default for Config {
    /// The defaults from the PRD.
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            auto_switch: true,
            weak_signal_dbm: -75,
            bad_ping_ms: 300,
            bad_loss_pct: 10,
            notify_on_switch: false,
            language: Language::En,
            ping_targets: default_ping_targets(),
            networks: Vec::new(),
        }
    }
}

fn default_ping_targets() -> Vec<IpAddr> {
    vec![
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
    ]
}

impl Config {
    /// Whether `ssid` is one of the user's networks and turned on.
    pub fn is_enabled(&self, ssid: &str) -> bool {
        self.enabled_networks().any(|network| network.ssid == ssid)
    }

    /// The user's turned-on networks, in priority order.
    pub fn enabled_networks(&self) -> impl Iterator<Item = &Network> {
        self.networks.iter().filter(|network| network.enabled)
    }

    /// Reads settings from `path`.
    ///
    /// - No file: defaults, [`Origin::FirstRun`]. Nothing is written.
    /// - Not valid TOML: the file is renamed to `config.toml.bak` and defaults
    ///   are used, [`Origin::Recovered`].
    /// - Valid TOML with bad values: those values use their defaults and are
    ///   listed in [`Loaded::notes`].
    ///
    /// Errors only when the file exists but can't be read or set aside (for
    /// example, no permission).
    pub fn load(path: &Path) -> io::Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    config: Config::default(),
                    origin: Origin::FirstRun,
                    notes: Vec::new(),
                });
            }
            Err(err) => return Err(err),
        };

        let parsed = String::from_utf8(bytes)
            .map_err(|err| err.to_string())
            .and_then(|text| Config::parse(&text).map_err(|err| err.to_string()));

        match parsed {
            Ok((config, notes)) => Ok(Loaded {
                config,
                origin: Origin::File,
                notes,
            }),
            Err(error) => {
                let backup = backup_path(path);
                fs::rename(path, &backup)?;
                Ok(Loaded {
                    config: Config::default(),
                    origin: Origin::Recovered { backup, error },
                    notes: Vec::new(),
                })
            }
        }
    }

    /// Writes settings to `path`, creating its folder if needed. The file is
    /// written to a temporary name first and then renamed, so a crash
    /// mid-write never leaves a half-written file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("toml.tmp");
        fs::write(&temp, self.to_toml())?;
        fs::rename(&temp, path)
    }

    /// The settings as TOML text, as [`Config::save`] writes them.
    pub fn to_toml(&self) -> String {
        let current = Config {
            version: CURRENT_VERSION,
            ..self.clone()
        };
        let body = toml::to_string(&current).expect("settings always serialize to TOML");
        format!("{HEADER}{body}")
    }

    /// Parses settings from TOML text, forgiving bad values. Fails only if the
    /// text isn't valid TOML.
    pub fn parse(text: &str) -> Result<(Config, Vec<String>), toml::de::Error> {
        let mut table: Table = text.parse()?;
        let mut notes = Vec::new();

        let version = match table.get("version") {
            // Hand-written files may leave it out.
            None => CURRENT_VERSION,
            Some(value) => match value.as_integer().map(u32::try_from) {
                Some(Ok(version)) => version,
                _ => {
                    notes.push(format!("`version` should be a whole number; reading the file as version {CURRENT_VERSION}"));
                    CURRENT_VERSION
                }
            },
        };
        if version > CURRENT_VERSION {
            notes.push(format!(
                "settings were saved by a newer version of the app (format {version}); settings this version doesn't know are ignored"
            ));
        }
        migrate(&mut table, version);

        let defaults = Config::default();
        let mut config = Config {
            version: CURRENT_VERSION,
            auto_switch: field(&table, "auto_switch", &mut notes).unwrap_or(defaults.auto_switch),
            weak_signal_dbm: field(&table, "weak_signal_dbm", &mut notes)
                .unwrap_or(defaults.weak_signal_dbm),
            bad_ping_ms: field(&table, "bad_ping_ms", &mut notes).unwrap_or(defaults.bad_ping_ms),
            bad_loss_pct: field(&table, "bad_loss_pct", &mut notes)
                .unwrap_or(defaults.bad_loss_pct),
            notify_on_switch: field(&table, "notify_on_switch", &mut notes)
                .unwrap_or(defaults.notify_on_switch),
            language: field(&table, "language", &mut notes).unwrap_or(defaults.language),
            ping_targets: ping_targets(&table, &mut notes).unwrap_or(defaults.ping_targets),
            networks: networks(&table, &mut notes),
        };

        for key in table.keys() {
            if !KNOWN_KEYS.contains(&key.as_str()) {
                notes.push(format!("unknown setting `{key}` was ignored"));
            }
        }

        notes.extend(config.normalize());
        Ok((config, notes))
    }

    /// Brings values into their allowed ranges and tidies the network list.
    /// Returns a note for each change.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut notes = Vec::new();

        let weak = self
            .weak_signal_dbm
            .clamp(*WEAK_SIGNAL_RANGE.start(), *WEAK_SIGNAL_RANGE.end());
        if weak != self.weak_signal_dbm {
            notes.push(format!(
                "`weak_signal_dbm` = {} is outside {}..={}; using {weak}",
                self.weak_signal_dbm,
                WEAK_SIGNAL_RANGE.start(),
                WEAK_SIGNAL_RANGE.end()
            ));
            self.weak_signal_dbm = weak;
        }

        let ping = self
            .bad_ping_ms
            .clamp(*BAD_PING_RANGE.start(), *BAD_PING_RANGE.end());
        if ping != self.bad_ping_ms {
            notes.push(format!(
                "`bad_ping_ms` = {} is outside {}..={}; using {ping}",
                self.bad_ping_ms,
                BAD_PING_RANGE.start(),
                BAD_PING_RANGE.end()
            ));
            self.bad_ping_ms = ping;
        }

        let loss = self
            .bad_loss_pct
            .clamp(*BAD_LOSS_RANGE.start(), *BAD_LOSS_RANGE.end());
        if loss != self.bad_loss_pct {
            notes.push(format!(
                "`bad_loss_pct` = {} is outside {}..={}; using {loss}",
                self.bad_loss_pct,
                BAD_LOSS_RANGE.start(),
                BAD_LOSS_RANGE.end()
            ));
            self.bad_loss_pct = loss;
        }

        if self.ping_targets.is_empty() {
            notes.push("`ping_targets` is empty; using 1.1.1.1 and 8.8.8.8".into());
            self.ping_targets = default_ping_targets();
        }

        let mut seen = Vec::with_capacity(self.networks.len());
        self.networks.retain(|network| {
            if network.ssid.is_empty() {
                notes.push("a network with an empty name was removed".into());
                return false;
            }
            if seen.contains(&network.ssid) {
                notes.push(format!(
                    "network `{}` was listed twice; kept the first",
                    network.ssid
                ));
                return false;
            }
            seen.push(network.ssid.clone());
            true
        });

        notes
    }
}

const KNOWN_KEYS: &[&str] = &[
    "version",
    "auto_switch",
    "weak_signal_dbm",
    "bad_ping_ms",
    "bad_loss_pct",
    "notify_on_switch",
    "language",
    "ping_targets",
    "networks",
];

/// Upgrades a settings table saved by an older version, one format at a time.
///
/// Version 1 is the first format, so there is nothing to upgrade yet. When the
/// format changes: bump [`CURRENT_VERSION`] and add a step here that rewrites
/// the old keys, e.g. `if from < 2 { rename "threshold" to "weak_signal_dbm" }`.
fn migrate(_table: &mut Table, _from: u32) {}

/// Reads one setting, or notes why it was ignored.
fn field<T: DeserializeOwned>(table: &Table, key: &str, notes: &mut Vec<String>) -> Option<T> {
    let value = table.get(key)?;
    match value.clone().try_into() {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            notes.push(format!(
                "`{key}` was ignored ({}); using the default",
                err.message()
            ));
            None
        }
    }
}

/// Reads `ping_targets`, keeping the addresses that parse.
fn ping_targets(table: &Table, notes: &mut Vec<String>) -> Option<Vec<IpAddr>> {
    let value = table.get("ping_targets")?;
    let Some(items) = value.as_array() else {
        notes.push("`ping_targets` should be a list of IP addresses; using the default".into());
        return None;
    };
    let targets = items
        .iter()
        .filter_map(|item| match item.as_str().map(str::parse::<IpAddr>) {
            Some(Ok(addr)) => Some(addr),
            _ => {
                notes.push(format!(
                    "ping target {item} isn't an IP address and was ignored"
                ));
                None
            }
        })
        .collect();
    Some(targets)
}

/// Reads the `[[networks]]` list, skipping entries that don't parse.
fn networks(table: &Table, notes: &mut Vec<String>) -> Vec<Network> {
    let Some(value) = table.get("networks") else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        notes.push("`networks` should be a list of [[networks]] entries; ignored".into());
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item.clone().try_into::<Network>() {
            Ok(network) => Some(network),
            Err(err) => {
                notes.push(format!(
                    "network #{} was ignored ({})",
                    index + 1,
                    err.message()
                ));
                None
            }
        })
        .collect()
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example from docs/TECHNICAL-DESIGN.md.
    const DESIGN_EXAMPLE: &str = r#"
version = 1
auto_switch = true
weak_signal_dbm = -75
bad_ping_ms = 300
bad_loss_pct = 10
notify_on_switch = false
language = "en"            # "en" | "id"
ping_targets = ["1.1.1.1", "8.8.8.8"]

[[networks]]               # order = priority
ssid = "Dorm-5G"
enabled = true

[[networks]]
ssid = "Dorm-2.4"
enabled = true
"#;

    fn parse(text: &str) -> (Config, Vec<String>) {
        Config::parse(text).expect("valid TOML")
    }

    #[test]
    fn defaults_match_the_prd() {
        let config = Config::default();
        assert!(config.auto_switch);
        assert_eq!(config.weak_signal_dbm, -75);
        assert_eq!(config.bad_ping_ms, 300);
        assert_eq!(config.bad_loss_pct, 10);
        assert!(!config.notify_on_switch);
        assert_eq!(config.language, Language::En);
        assert_eq!(
            config.ping_targets,
            [
                "1.1.1.1".parse::<IpAddr>().unwrap(),
                "8.8.8.8".parse().unwrap()
            ]
        );
        assert!(config.networks.is_empty());
    }

    #[test]
    fn reads_the_design_doc_example_without_notes() {
        let (config, notes) = parse(DESIGN_EXAMPLE);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(
            config.networks,
            [Network::new("Dorm-5G"), Network::new("Dorm-2.4")]
        );
        assert_eq!(
            config,
            Config {
                networks: config.networks.clone(),
                ..Config::default()
            }
        );
    }

    #[test]
    fn saved_text_reads_back_the_same() {
        let config = Config {
            auto_switch: false,
            weak_signal_dbm: -70,
            bad_ping_ms: 250,
            bad_loss_pct: 5,
            notify_on_switch: true,
            language: Language::Id,
            ping_targets: vec!["9.9.9.9".parse().unwrap()],
            networks: vec![
                Network::new("Kos Melati"),
                Network {
                    ssid: "Kos Melati 5G".into(),
                    enabled: false,
                },
            ],
            ..Config::default()
        };
        let text = config.to_toml();
        assert!(text.starts_with("# Thunder WiFi Switcher settings."));
        let (back, notes) = parse(&text);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(back, config);
    }

    #[test]
    fn missing_version_is_read_as_current() {
        let (config, notes) = parse("weak_signal_dbm = -70\n");
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(config.version, CURRENT_VERSION);
        assert_eq!(config.weak_signal_dbm, -70);
    }

    #[test]
    fn newer_version_is_read_with_a_note() {
        let (config, notes) = parse("version = 99\nweak_signal_dbm = -70\nfuture_thing = 1\n");
        assert_eq!(config.weak_signal_dbm, -70);
        assert!(notes.iter().any(|note| note.contains("newer version")));
        assert!(notes.iter().any(|note| note.contains("future_thing")));
    }

    #[test]
    fn a_bad_value_falls_back_without_losing_the_rest() {
        let text = r#"
weak_signal_dbm = "very weak"
language = "fr"

[[networks]]
ssid = "Dorm-5G"
"#;
        let (config, notes) = parse(text);
        assert_eq!(config.weak_signal_dbm, -75);
        assert_eq!(config.language, Language::En);
        assert_eq!(config.networks, [Network::new("Dorm-5G")]);
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn a_bad_network_entry_is_skipped() {
        let text = r#"
[[networks]]
ssid = "Good"

[[networks]]
enabled = true

[[networks]]
ssid = "Also good"
enabled = false
"#;
        let (config, notes) = parse(text);
        assert_eq!(
            config.networks,
            [
                Network::new("Good"),
                Network {
                    ssid: "Also good".into(),
                    enabled: false
                }
            ]
        );
        assert!(notes[0].contains("network #2"), "{notes:?}");
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let (config, notes) =
            parse("weak_signal_dbm = -95\nbad_ping_ms = 10\nbad_loss_pct = 150\n");
        assert_eq!(config.weak_signal_dbm, -80);
        assert_eq!(config.bad_ping_ms, 50);
        assert_eq!(config.bad_loss_pct, 100);
        assert_eq!(notes.len(), 3, "{notes:?}");
    }

    #[test]
    fn duplicate_and_empty_networks_are_removed() {
        let text = r#"
[[networks]]
ssid = "A"
[[networks]]
ssid = ""
[[networks]]
ssid = "A"
enabled = false
[[networks]]
ssid = "B"
"#;
        let (config, notes) = parse(text);
        assert_eq!(config.networks, [Network::new("A"), Network::new("B")]);
        assert_eq!(notes.len(), 2, "{notes:?}");
    }

    #[test]
    fn invalid_ping_targets_are_dropped() {
        let (config, notes) = parse(r#"ping_targets = ["9.9.9.9", "dns.google", 5]"#);
        assert_eq!(config.ping_targets, ["9.9.9.9".parse::<IpAddr>().unwrap()]);
        assert_eq!(notes.len(), 2, "{notes:?}");

        let (config, notes) = parse("ping_targets = []");
        assert_eq!(config.ping_targets, default_ping_targets());
        assert_eq!(notes.len(), 1, "{notes:?}");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(loaded.origin, Origin::FirstRun);
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn save_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ThunderWiFi").join("config.toml");
        let config = Config {
            networks: vec![Network::new("Dorm-5G")],
            ..Config::default()
        };
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.origin, Origin::File);
        assert_eq!(loaded.config, config);
        assert!(loaded.notes.is_empty());
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn save_replaces_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save(&path).unwrap();
        let changed = Config {
            weak_signal_dbm: -68,
            ..Config::default()
        };
        changed.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().config, changed);
    }

    #[test]
    fn unreadable_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this is [not toml").unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.config, Config::default());
        let Origin::Recovered { backup, .. } = &loaded.origin else {
            panic!("expected Recovered, got {:?}", loaded.origin);
        };
        assert_eq!(backup, &dir.path().join("config.toml.bak"));
        assert_eq!(fs::read_to_string(backup).unwrap(), "this is [not toml");
        assert!(!path.exists());
    }

    #[test]
    fn non_utf8_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert!(matches!(loaded.origin, Origin::Recovered { .. }));
    }
}
