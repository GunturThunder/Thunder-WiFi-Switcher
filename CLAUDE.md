# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Thunder WiFi Switcher is a free, MIT-licensed menu bar / tray app for macOS and Windows that moves the laptop to the user's saved Wi-Fi network with the best internet, not just the strongest signal. It's built with Rust and Tauri v2, and the settings window uses Svelte 5 and TypeScript. The audience is students in dorms and boarding houses (kos). Write user-facing text, comments and explanations in plain English: many users and contributors aren't native speakers.

The source of truth is `docs/PRD.md` (what and why), `docs/TECHNICAL-DESIGN.md` (how; keep it in step with the code) and `docs/ROADMAP.md` (phases). Check the roadmap and `CHANGELOG.md` to see what's done and what's next.

## Commands

Rust comes from `rustup`; `rust-toolchain.toml` picks stable with rustfmt and clippy. The settings window needs Node.js LTS.

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings        # CI treats warnings as errors
cargo test --workspace
cargo test -p thunder-core --test scenarios walking_away      # one scenario test
cargo test -p thunder-core engine::tests::user_switch         # unit tests by name prefix
cargo test -p wifi-macos -- --ignored                         # Keychain round trip (writes to the login Keychain)
RUSTDOCFLAGS="-D warnings" cargo doc -p thunder-core --no-deps
```

These checks work from macOS. CI runs the real Linux and Windows builds.

```bash
rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-gnu      # once
cargo clippy --target x86_64-pc-windows-msvc -p thunder-core -p wifi-windows -p wifi-macos --all-targets -- -D warnings
cargo clippy --target x86_64-unknown-linux-gnu --workspace --exclude thunder-wifi-switcher --all-targets -- -D warnings
```

The app crate (`thunder-wifi-switcher`) can't be checked for Windows from a Mac: Tauri's build script needs the Windows resource compiler. Re-read its `#[cfg(...)]` branches by hand, and rely on the "App (Windows)" CI job.

Settings window (from `app/`):

```bash
npm ci
npm run check            # svelte-check + tsc
npm run build            # Vite → app/dist
npm run dev              # browser only, with the pretend backend; add ?platform=windows, ?firstrun or ?notifications=blocked
```

Running the app:

```bash
cd app && npm run tauri dev -- -- --simulate --settings   # pretend networks, any OS
scripts/run-app-macos.sh [--simulate] [--settings]          # real Wi-Fi on macOS: builds a debug .app and opens it
scripts/probe-macos.sh status|permission|scan|known|save <ssid>|forget <ssid>|connect <ssid>|ping|watch
cargo run -p wifi-windows --example probe -- status|permission|scan|known|connect <ssid>|ping|watch
```

- Logs: `~/Library/Logs/ThunderWiFi/thunderwifi.log` (macOS), `%LOCALAPPDATA%\ThunderWiFi\logs\` (Windows).
- Settings: `~/Library/Application Support/ThunderWiFi/config.toml`. `--simulate` uses `config-simulate.toml` instead.
- macOS system log: use `/usr/bin/log`, because zsh has a `log` builtin. For example, `/usr/bin/log show --last 10m --predicate 'process == "locationd" AND eventMessage CONTAINS[c] "thunderwifi"'` shows why a Location prompt didn't appear.

## Architecture

```
crates/thunder-core   all decisions; no OS code; builds and tests on Linux
crates/wifi-macos     CoreWLAN, Keychain, CoreLocation, SystemConfiguration, NSWorkspace, UserNotifications, ICMP socket
crates/wifi-windows   wlanapi, IP Helper, power notifications, IcmpSendEcho
app/src-tauri         Tauri shell: worker thread, tray, commands, plugins
app/ui                settings window (Vite root is app/ui; package.json and vite.config.ts are in app/)
```

Each backend crate is `#![cfg(target_os = "macos")]` or `#![cfg(windows)]` at the crate root, so it compiles to nothing elsewhere.

### Data flow

1. Every 5 s, the worker thread (`app/src-tauri/src/worker.rs`, the only thread that touches the Wi-Fi radio) calls `Driver::tick`.
2. `Driver::tick` (`thunder-core/src/driver.rs`):
   1. `read_sample` reads the link through `WifiBackend`.
   2. While connected and not paused, `quality::Prober::probe` pings the router and the public targets in parallel.
   3. `quality::verdict` turns the results into an `Internet` value, carried in `Sample::Connected(link, internet)`.
   4. `Engine::step(now, Event::Sample)` decides what to do.
3. The engine returns an `Action` (`Scan`, `Connect`, `Notify`). The driver carries it out against the backend and feeds `ScanDone` / `ConnectDone` back until the action is `None` or `Notify`.
4. The worker builds a `Status` (`status.rs`). When it changes, the shell redraws the tray (`tray.rs`) and emits a `status` event to the window. If notifications are on, each automatic switch also becomes an `Alert` (`status.rs`), shown with the notification plugin; switches the user made don't. Menu commands (`Command::SwitchTo`, `SetConfig`, `Sleep`, `Wake`, `Scan`) reach the worker over a channel.

### Core modules (`thunder-core`)

- **`wifi.rs`:** `WifiBackend` trait (current, scan, connect, known_networks, gateway, permission, request_permission, powered, ping, tcp_probe), with `Link`, `ScanResult`, `Permission` and `Error`. All network I/O, including probes, goes through this trait, so tests never touch the real network. `Arc<T>` forwards every method; add new methods there too.
- **`engine.rs`:** a pure state machine. It reads no clock (`now` is passed in) and does no I/O.
  - States: `Watching` → `Scanning` → `Switching` → `Cooldown`, plus `Backoff` (30 s doubling to 300 s, reset by a good sample) and `Paused(User | Sleep)`.
  - `Reason`: `WeakSignal`, `Disconnected`, `BadInternet`, `BadLink`, `User`.
  - Leaving a network for bad internet, or failing to join it, marks it in `avoid_until` for 10 minutes.
- **`rules.rs`:** every PRD number as a constant, plus `assess()` (is this sample a problem?) and `choose_network()` (which candidate wins: `Requirement::StrongerThan` for weak signal (8 dB margin), `AtLeast(weak_signal_dbm)` for bad internet, `Any` when disconnected; list priority wins within 3 dB).
- **`quality.rs`:** 6-result windows per target, a median and loss figure for each, judged only from 3 results onwards.
  - The first 2 rounds on a network (`GRACE_ROUNDS`) probe nothing, and loss needs at least 2 lost probes (`MIN_LOST`), so joining a network or one lost ping never causes a switch.
  - A failed ping is retried right away over TCP port 443 (a refusal counts as an answer). After 3 such retries the target stays on TCP for that network.
  - The internet is bad only if **every** public target is bad.
  - The router only counts once it has answered on this network.
- **`config.rs`:** TOML settings. `Config::parse` is forgiving: one bad value falls back to its default with a note. A file that isn't valid TOML is renamed to `config.toml.bak`. Saves are atomic. When adding a setting, update the struct, `Default`, `KNOWN_KEYS`, `parse()`, `normalize()` if it has a range, and the tests.
- **`fake.rs`:** `FakeBackend` for tests and `--simulate`. It can script signal, outages (`set_internet`), silent routers, blocked ICMP and connect failures.

### App shell (`app/src-tauri/src`)

- **`lib.rs`:**
  - setup: load the config, pick the backend (`backend.rs`: real, simulated, or `Unsupported` on Linux), build the tray, spawn the worker, `app.manage(AppState)`, start the sleep watcher
  - `apply_config()`: every settings change goes through it (normalize, save, send to the worker, redraw the tray, emit `config`). Turning notifications on sends a test one.
- **`commands.rs`:** the only API the window can call. Passwords only travel into the Keychain, never back out. `open_link` only opens a fixed list of URLs. `switch_to` ("Join now") waits for the worker's result and turns a failure into a `JoinError` (`status.rs`: a `kind` for the fix button plus a plain message).
- **`simulate.rs`:** a 6-minute loop. Walking away gives a weak-signal switch; Dorm-2.4's internet outage gives a bad-internet switch.

### Settings window (`app/ui/src`)

- `lib/api.ts` uses Tauri's `invoke` inside the app and `lib/mock.ts` in a plain browser.
- `lib/store.svelte.ts` holds the state (Svelte 5 runes), including the current tab and the notification and open-at-login state. Every change saves immediately; there's no Save button.
- `lib/SetupGuide.svelte` is the setup guide (Location, networks, notifications, open at login). It opens while `app.setupNeeded` and from About → Setup guide. Keep its quotes of system prompts in step with what macOS and Windows really show.
- Types in `lib/types.ts` mirror the Rust serde output and stay snake_case.

## Platform gotchas (all learned the hard way)

macOS:
- **No prompt for terminal binaries.** macOS never shows the Location prompt to a plain binary started from a terminal, and that includes `tauri dev`. That's why `scripts/probe-macos.sh` wraps the probe in an app bundle launched with `open`, and why `scripts/run-app-macos.sh` builds a real `.app`.
- **Hardened runtime needs the entitlement.** Tauri signs with the hardened runtime, so `app/src-tauri/Entitlements.plist` must keep `com.apple.security.personal-information.location`. Without it, the prompt is silently dropped.
- **Permission status arrives late.** It's only known after a `CLLocationManager` has run on the main thread; `MacBackend::new()` starts one. Decide `PermissionDenied` from CoreWLAN returning nil names, not from the status.
- **Rebuilds reset trust.** Ad-hoc-signed builds lose Location permission and Keychain trust after every rebuild. That's expected until Developer ID signing (Phase 5).
- **"Resource busy" (EBUSY).** CoreWLAN returns it while macOS is busy itself, so calls retry twice (`retry_if_busy`).
- **Notifications need the user's OK.** The first notification makes macOS ask "would like to send notifications". If the user picks Don't Allow (or closes the box), later notifications go silently to Notification Center with no banner. macOS asks each app only once; after a no, only System Settings can change it. The plugin can't read the choice (it always says granted on desktop), so `wifi-macos/src/notifications.rs` reads and asks with `UNUserNotificationCenter`, and the General tab shows a warning with a link to the app's notification settings. That framework stops the process outside a real `.app` (`tauri dev`, tests), so it's only called when the main bundle path ends in `.app`. `usernoted` logs each answer: `Authorization set for io.github.gunturthunder.thunderwifi to allow: NO`.
- **Password prompts need a terminal.** An app launched with `open` has no controlling terminal, so `probe-macos.sh` runs `save`, `forget` and `known` directly.

Windows:
- **Location is off → access denied.** On Windows 11 24H2+, wlanapi returns `ERROR_ACCESS_DENIED` when location is off for desktop apps. That maps to `PermissionDenied`, and `request_permission` opens `ms-settings:privacy-location`.
- **Passwords:** the app never stores them on Windows; `connect` uses the profile Windows already saved.
- **Don't use `WlanReasonCodeToString`.** The `windows` crate binds its output buffer as read-only.

## Conventions

- **No dead code, no unused dependencies.** CI runs Clippy with `-D warnings`, and Cargo warns about unused dependencies. Code that only one OS uses must be `#[cfg]`-gated, or it becomes dead code on the other OS and breaks that CI job (`Command::Sleep`/`Wake` once did). Don't add placeholders "for later"; write it in the phase that uses it.
- **Keep the layers separate.** Decisions go in `thunder-core` (pure, unit-tested); OS calls go in the backend crates; the shell only wires them together. New behaviour gets a scenario test in `crates/thunder-core/tests/scenarios.rs` (the `Sim` harness ticks the real `Driver` against `FakeBackend` 5 s at a time).
- **Prove new rules are tested.** Break the rule on purpose, run `cargo test --no-fail-fast`, confirm the tests fail, then restore it.
- **`unsafe` needs a `// SAFETY:` comment.**
- **Plain words for users.** Comments, docs, log lines, menu text and error messages are written for non-experts. Logs contain network names, signal and ping only, never passwords.
- **Docs change with the code.** When behaviour changes, update `docs/TECHNICAL-DESIGN.md`, add a line under `Unreleased` in `CHANGELOG.md` for anything users notice, and update `PRIVACY.md` if what's stored or sent changes.
- **Dependabot ignores TypeScript 7** (`.github/dependabot.yml`) until `svelte-check` supports it.

## Workflow

- **Branches and PRs.** One branch and one PR per phase or change; never push to `main`. `main` is protected: a PR is required, and the checks `Core and UI (Linux)`, `App (macOS)` and `App (Windows)` must pass. PRs are squash-merged.
- **Commits.** Messages are in the imperative, split into logical commits (core / backends / app / docs). The body says why.
- **Before pushing,** run fmt, Clippy (macOS, plus the Windows and Linux cross-checks above), `cargo test --workspace`, and `npm run check` in `app/`.
- **Releases** (maintainer only): bump the numeric version in `Cargo.toml` in a PR, then tag `main` (`v0.5.0`, or `v0.5.0-beta.1` for a pre-release). `release.yml` builds the installers into a draft release. Run `docs/TESTING.md` on both OSes, then publish. Details are in CONTRIBUTING.md → Releasing.
- **Testing on real hardware.** On macOS, use the probe and `run-app-macos.sh`. Beyond CI, the Windows backend needs a real Windows laptop: use the Windows probe there.
