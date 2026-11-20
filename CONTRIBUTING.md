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

## Running the app

```bash
cd app
npm install
npm run tauri dev
```

The app lives in the menu bar (macOS) or system tray (Windows); there's no Dock icon or main window. Choose **Settings…** from its menu to open the settings window. To open it straight away while developing, pass `--settings`:

```bash
npm run tauri dev -- -- --settings
```

To work on the settings window in a normal browser (no Tauri features), run `npm run dev` in `app/` and open http://localhost:1420.

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

## Security issues

Please don't report security problems in public issues. See [SECURITY.md](SECURITY.md).

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
