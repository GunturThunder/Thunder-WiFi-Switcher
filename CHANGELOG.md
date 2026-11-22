# Changelog

All notable changes to Thunder WiFi Switcher are listed here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Release workflow: pushing a version tag builds the macOS `.dmg` (Apple Silicon and Intel) and Windows `.msi` (x64 and ARM64) and attaches them to a draft GitHub Release.
- Install instructions in the README, and a manual release checklist in `docs/TESTING.md`.
- Internet-quality checks: every 5 seconds the app pings your router, 1.1.1.1 and 8.8.8.8 (falling back to a port-443 connection when ping is blocked) and switches away from a network whose internet is down or slow, or whose router stops responding, even with a strong signal. A network left for bad internet is skipped for 10 minutes.
- Ping in the menu bar label (`-58 · 24ms`), the menu and the settings window; "no internet" when the checks fail.
- `ping` command in both probe tools; `--simulate` now includes an internet outage.
- Windows Wi-Fi backend (`wifi-windows`): reads the current network and signal, scans, joins networks with the profiles Windows has saved, lists them, finds the router, detects when location is off for desktop apps (Windows 11 24H2+), and reports sleep and resume. The app now uses it on Windows.
- Windows `probe` tool (`cargo run -p wifi-windows --example probe`).
- The app now runs the switching engine on real Wi-Fi (macOS): menu bar icon with signal level and an optional `-58` label, a menu with status, last switch, auto-switch toggle and "Switch to", and logs in `~/Library/Logs/ThunderWiFi/`.
- Settings window: add networks from the ones nearby, save their passwords in the Keychain, drag them into priority order, set the weak-signal level, open at login, and a first-run card for Location permission.
- `--simulate` mode with pretend networks, and `scripts/run-app-macos.sh` to try a debug build on real Wi-Fi.
- macOS Wi-Fi backend (`wifi-macos`): reads the current network and signal, scans, joins networks with passwords from the Keychain, lists known networks, finds the router, asks for Location permission, and reports sleep and wake.
- `probe` tool for trying the macOS backend on real Wi-Fi (`scripts/probe-macos.sh`).
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

### Fixed

- Opening the settings window no longer makes macOS ask for your Keychain password once per saved network: the app now checks that a password is saved without reading it.
