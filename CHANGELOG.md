# Changelog

All notable changes to Thunder WiFi Switcher are listed here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- macOS Wi-Fi backend (`wifi-macos`): reads the current network and signal, scans, joins networks with passwords from the Keychain, lists known networks, finds the router, asks for Location permission, and reports sleep and wake.
- `probe` tool for trying the macOS backend on real Wi-Fi (`cargo run -p wifi-macos --example probe`).
- Switching engine in `thunder-core`: watches signal every 5 seconds, switches after 15 seconds of weak signal to a saved network at least 8 dB stronger, reconnects straight away when Wi-Fi drops, and backs off from 30 seconds to 5 minutes when nothing better is in range.
- Settings file (TOML) with defaults, forgiving loading of hand-edited files, and recovery from unreadable files.
- Fake Wi-Fi backend and scenario tests (walking away, flapping signal, two equal networks, dropped connection, sleep and wake).
- Repository setup: PRD, technical design and roadmap in `docs/`, app icons, and the original Swift prototype for reference.
- Community files: README, MIT license, code of conduct, contributing guide, security policy and privacy policy.
- Issue forms (bug report, feature request, network report) and a pull request template.
- CI on Linux, macOS and Windows: formatting, Clippy, tests, UI checks and an app build.
- Cargo workspace with `thunder-core`, `wifi-macos` and `wifi-windows` crates (empty for now).
- Tauri v2 app skeleton: menu bar / tray icon with a basic menu, and an empty settings window.
- Menu bar template icons (0–3 arcs and disconnected) in `assets/icons/menubar`; the macOS menu bar now uses them.
