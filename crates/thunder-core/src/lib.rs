//! Shared core of Thunder WiFi Switcher.
//!
//! Every switching decision is made here. This crate never calls OS APIs, so it
//! builds and tests on any machine, including Linux CI. The platform crates
//! (`wifi-macos`, `wifi-windows`) do the Wi-Fi work behind the [`WifiBackend`]
//! trait.
//!
//! - [`engine`]: the state machine. Feed it [`Event`]s, get [`Action`]s back.
//! - [`rules`]: the PRD's thresholds and the "which network wins" logic.
//! - [`quality`]: ping checks of the router and public servers.
//! - [`config`]: settings, stored as TOML.
//! - [`driver`]: runs the engine against a backend.
//! - [`fake`]: a pretend backend for tests and simulation.
//!
//! See `docs/TECHNICAL-DESIGN.md` for how the pieces fit together.

#![forbid(unsafe_code)]

pub mod config;
pub mod driver;
pub mod engine;
pub mod fake;
pub mod quality;
pub mod rules;
pub mod wifi;

pub use config::{Config, Language, Loaded, Network, Origin};
pub use driver::Driver;
pub use engine::{Action, Engine, Event, Notice, PauseReason, Reason, Sample, State};
pub use quality::{Internet, Quality};
pub use wifi::{Error, Link, Permission, Result, ScanResult, WifiBackend};

/// Version of the core, shown in the About tab.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
