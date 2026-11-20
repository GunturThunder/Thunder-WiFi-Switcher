//! Shared core of Thunder WiFi Switcher.
//!
//! Every switching decision is made here. This crate never calls OS APIs, so it
//! builds and tests on any machine, including Linux CI. The platform crates
//! (`wifi-macos`, `wifi-windows`) do the Wi-Fi work behind the `WifiBackend`
//! trait.
//!
//! Phase 1 of the roadmap adds the engine, rules, config and quality prober;
//! see `docs/TECHNICAL-DESIGN.md`.

#![forbid(unsafe_code)]

/// Version of the core, shown in the About tab.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_matches_workspace() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        assert!(!VERSION.is_empty());
    }
}
