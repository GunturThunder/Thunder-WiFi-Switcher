# Changelog

All notable changes to Thunder WiFi Switcher are listed here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Places:** give your networks a place, like Dorm or Office. The Networks tab groups them by place, **Switch to** in the menu shows only the place you're in, and the top of the settings window says which place you're at. New networks go into the place you're in. Places never stop the app from switching: walking from one place to the next still works.

## [0.5.0] - 2026-11-22

The first public release, a beta for macOS 14 or later and Windows 10 22H2 or Windows 11 (x64 and ARM64).

### Added

- **Switching to the network with the best internet.** Every 5 seconds the app checks the current Wi-Fi network and moves to another of your saved networks:
  - after 15 seconds of weak signal, to a network at least 8 dB stronger;
  - straight away when Wi-Fi drops;
  - when the internet is down or slow, or the router stops responding, even with a strong signal. A network left for bad internet is skipped for 10 minutes.
  - When nothing better is in range, it waits 30 seconds before looking again, doubling up to 5 minutes. Checks pause while the laptop sleeps.
- **Internet checks.** A tiny ping to your router, 1.1.1.1 and 8.8.8.8, or a port-443 connection when ping is blocked. The internet only counts as bad when every public server fails, one lost ping never counts, and the first 10 seconds on a network aren't judged. When the internet is judged bad, the log says what each server did.
- **Menu bar / tray icon** with the signal level and an optional label (`-58 · 24ms`), and a menu with the status, the last switch, **Auto-switch** and **Switch to**.
- **Settings window:**
  - add networks from the ones nearby, and drag them into priority order;
  - **Join now**, which says why joining failed (not in range, no password saved, wrong password, Location needed, Wi-Fi off, no answer, or a network that needs a user name) and offers the fix;
  - set the weak-signal level, the bad ping and loss limits, and the ping servers;
  - open at login.
  - On macOS, each network's password is saved once in your Keychain; the app only checks that it's there, so opening the window never asks for your Mac password. On Windows, the app uses the passwords Windows has saved.
- **Setup guide** on first run, and from About any time: what the app asks for, why, and what macOS or Windows will show. It covers Location, your networks and their passwords, notifications, and open at login. After you allow Location, the Nearby list fills in by itself.
- **Switch notifications** (off by default): when the app switches by itself, a notification says where it went and why. On macOS the app asks for permission when you turn them on, and says so if macOS is blocking them.
- **Automatic updates:** the app looks for a new version twice a day, downloads it in the background, and installs it when you choose **Restart to Update** in its menu. You can turn the checks off in General. On macOS, macOS asks for Location again after an update, because the app isn't signed by Apple; the setup guide opens by itself to help.
- **Privacy:** the app stores only its settings file, your Wi-Fi passwords (macOS Keychain) and a log of network names, signal and ping. It sends nothing but the pings and the update check. See [PRIVACY.md](PRIVACY.md).
- **For contributors:** pretend networks (`--simulate`), probe tools for real Wi-Fi on macOS and Windows, scenario tests, CI on Linux, macOS and Windows, and a release workflow that builds the installers and signs the update files.

[Unreleased]: https://github.com/GunturThunder/Thunder-WiFi-Switcher/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/GunturThunder/Thunder-WiFi-Switcher/releases/tag/v0.5.0
