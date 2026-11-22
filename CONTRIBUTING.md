# Contributing to Thunder WiFi Switcher

Thanks for helping! This guide covers how to set up the project, how the code is organised, and how to get a change merged.

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Network reports.** If the app switches when it shouldn't (or doesn't when it should), file a [network report](https://github.com/GunturThunder/Thunder-WiFi-Switcher/issues/new?template=network-report.yml). Real readings from dorms, kos and shared houses are the most useful thing you can give us.
- **Bug reports and feature ideas** through the [issue forms](https://github.com/GunturThunder/Thunder-WiFi-Switcher/issues/new/choose). Check the [PRD](docs/PRD.md) first: some features are deliberately out of scope for v1.0.
- **Code.** Issues labelled `good first issue` are a good place to start. For anything bigger, open an issue first so we can agree on the approach before you spend time on it.
- **Testing on Windows.** We have fewer Windows testers than Mac ones.

## Project layout

```
crates/thunder-core/   switching engine, rules, config, quality checks (no OS code)
crates/wifi-macos/     CoreWLAN + Keychain backend (compiles only on macOS)
crates/wifi-windows/   wlanapi backend (compiles only on Windows)
app/src-tauri/         Tauri shell: tray, windows, commands
app/ui/                settings window (Svelte + TypeScript)
assets/icons/          app and tray icons
docs/                  PRD, technical design, roadmap
prototype/macos/       original Swift prototype (reference only)
```

All switching decisions live in `thunder-core`, which never calls OS APIs, so it builds and tests on any machine. The backends only talk to the Wi-Fi hardware. [docs/TECHNICAL-DESIGN.md](docs/TECHNICAL-DESIGN.md) explains the design.

## Setting up

### Everyone

- **Rust** (stable) through [rustup](https://rustup.rs). The repo's `rust-toolchain.toml` picks the right toolchain and installs `rustfmt` and `clippy` for you.
- **Node.js** current LTS from [nodejs.org](https://nodejs.org), for the settings window.

### macOS

- macOS 14 Sonoma or later.
- Xcode Command Line Tools: `xcode-select --install`.

### Windows

- Windows 10 22H2 or Windows 11.
- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload.
- WebView2, which is already installed on Windows 10 22H2 and Windows 11.
- Use the MSVC Rust toolchain (rustup's default on Windows).

### Linux

The app doesn't run on Linux, but `thunder-core` builds and tests there, so you can work on the engine:

```bash
cargo test -p thunder-core
```

## Trying the Windows Wi-Fi backend

The `probe` example runs the Windows backend on your real Wi-Fi, without the app (in a terminal, from the repo folder):

```bash
cargo run -p wifi-windows --example probe -- status           # current network, signal, router, location access
cargo run -p wifi-windows --example probe -- scan             # networks in range, strongest first
cargo run -p wifi-windows --example probe -- known            # networks Windows has saved
cargo run -p wifi-windows --example probe -- connect "Dorm-5G" # join one (it must be saved in Windows first)
cargo run -p wifi-windows --example probe -- ping             # router, 1.1.1.1, 8.8.8.8: ping and port 443
cargo run -p wifi-windows --example probe -- watch            # signal every 5 s; sleep the PC to see sleep/wake
```

On Windows 11 24H2 and later, Wi-Fi details count as location data. If `status` says location is blocked, run `probe permission` and turn on **Let desktop apps access your location**. The app itself: `cd app`, `npm install`, `npm run tauri dev`.

## Trying the macOS Wi-Fi backend

`scripts/probe-macos.sh` runs the macOS backend on your real Wi-Fi, without the app. It wraps the `probe` example in a small app bundle (`target/probe/Thunder WiFi Probe.app`), because macOS never shows the Location prompt to a plain command-line tool.

```bash
scripts/probe-macos.sh permission         # answer the Location prompt once
scripts/probe-macos.sh status             # current network, signal, router
scripts/probe-macos.sh scan               # networks in range, strongest first
scripts/probe-macos.sh save "Dorm-5G"     # store a password (asks for it)
scripts/probe-macos.sh connect "Dorm-5G"  # join it
scripts/probe-macos.sh ping               # router, 1.1.1.1, 8.8.8.8: ping and port 443
scripts/probe-macos.sh watch              # signal every 5 s; close the lid to see sleep/wake
```

macOS ties the permission to this exact build, so after the probe is rebuilt (for example after pulling new code) run `permission` again. Until it has been granted for the current build, `status` reports "not asked yet" and `scan` reports that permission is needed.

Passwords go in your login Keychain under the service `ThunderWiFi`; `scripts/probe-macos.sh forget "<name>"` removes one. The Keychain test is skipped by default because it writes to your Keychain (under `ThunderWiFi-test`, and cleans up): run it with `cargo test -p wifi-macos -- --ignored`.

## Running the app

The app lives in the menu bar (macOS) or system tray (Windows); there's no Dock icon or main window. Choose **Settings…** from its menu to open the settings window.

**With pretend networks** (no real switching, works on any Mac or PC): `--simulate` runs three networks whose signal changes on a 6-minute loop, plus an internet outage, so you can watch it switch on weak signal and on bad internet. It keeps its own settings file (`config-simulate.toml`).

```bash
cd app
npm install
npm run tauri dev -- -- --simulate --settings
```

**On your real Wi-Fi (macOS):** macOS only gives Location permission, and so network names, to apps opened as a real `.app`, not to `tauri dev`. This builds a debug `.app` and opens it:

```bash
scripts/run-app-macos.sh
```

Each rebuild is a new app to macOS, so it asks for Location permission again, and the Keychain may ask to let it read saved passwords (click **Always Allow**). Logs go to `~/Library/Logs/ThunderWiFi/thunderwifi.log`.

**Just the settings window**, in a normal browser with pretend data: run `npm run dev` in `app/` and open http://localhost:1420.

## Before you open a pull request

Run the same checks CI runs:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

```bash
cd app
npm run check
npm run build
```

On Linux, add `--exclude thunder-wifi-switcher` to the `clippy` and `test` commands, since the app crate only builds on macOS and Windows.

## Pull requests

- **Every change goes through a pull request**, and CI must be green before it's merged. This applies to maintainers too.
- Keep each pull request to one change. Small pull requests get reviewed faster.
- Explain what changed and why, and how you tested it (which OS, which networks).
- Add tests for engine rules: every rule in `thunder-core` should have a scripted test.
- If your change affects what the app stores or sends, update [PRIVACY.md](PRIVACY.md) in the same pull request.
- Add a line to the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md) for anything a user would notice.
- Write commit messages in the imperative ("Add Keychain store", not "Added Keychain store").

## Releasing (maintainers)

1. In a pull request, set the version in `Cargo.toml` (`[workspace.package]`) and move the `Unreleased` changelog entries under the new version.
   - Keep it plain numbers (`0.5.0`): Windows installers can't carry labels like `-beta`.
   - Every release needs a higher number than the last, betas too (`0.5.0`, then `0.5.1`, …). Installed apps compare these numbers to decide whether to update.
2. After it's merged, tag `main` and push the tag. A label after the number is fine for people (`v0.5.1-beta.2`), as long as the number matches `Cargo.toml`:

   ```bash
   git tag v0.5.0 && git push origin v0.5.0
   ```

3. `release.yml` builds the macOS `.dmg` (universal) and the Windows `.msi` (x64 and ARM64), signs the update files, and attaches everything, including `latest.json`, to a **draft** release. It stops if the tag doesn't match the version.
4. Run [docs/TESTING.md](docs/TESTING.md) with those installers on a Mac and a Windows laptop, then publish the draft. Don't tick "Set as a pre-release": installed apps only look at the latest release.

The update files are signed with the Tauri updater key, kept in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Only the maintainer has the private key (with a backup). If it's lost, installed apps can't update any more and need a manual reinstall; if it leaks, make a new key and rotate the secrets and the public key in `app/src-tauri/tauri.conf.json`.

A pull request that changes `release.yml` builds the same installers as workflow artifacts (on the PR's checks page), without releasing anything.

## Security issues

Please don't report security problems in public issues. See [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
