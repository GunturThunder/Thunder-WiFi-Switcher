# Thunder WiFi Switcher – Roadmap

Last updated: Nov 20, 2026

Seven phases take Thunder WiFi Switcher from an empty repo to v1.0, with three releases along the way: v0.1 (Mac alpha), v0.5 (Mac + Windows beta) and v1.0. Durations are estimates for one part-time developer working with Claude Code.

## Phases at a glance

| Phase | Weeks | Release gate after it |
| --- | --- | --- |
| 0 – Repo setup | 1 | |
| 1 – Core engine | 2–3 | |
| 2 – Platform backends | 4–6 | |
| 3 – Tray UI and settings | 7–8 | **v0.1 Mac alpha** (Mac, signal only) |
| 4 – Quality checks | 9–10 | **v0.5 beta** (Mac + Windows) |
| 5 – Packaging and release | 11–12 | |
| 6 – Community and v1.0 launch | 13–14 | **v1.0** (public launch) |

The Mac alpha ships first because the Mac is the main test machine; Windows joins at the v0.5 beta once the tester confirms it works.

## Phase 0 – Repo setup (week 1)

Done when: the repo has every community file and CI builds an empty Tauri app on macOS and Windows.

- [ ] Add `docs/` (PRD, technical design, this roadmap), `assets/icons/`, `prototype/macos/` — you
- [ ] README with icon, one-line pitch, status badge, "work in progress" note — Claude Code
- [ ] LICENSE (MIT), CODE_OF_CONDUCT, CONTRIBUTING, SECURITY, PRIVACY — Claude Code
- [ ] Issue templates (bug, feature, network report) and pull request template — Claude Code
- [ ] Cargo workspace: `crates/thunder-core`, `crates/wifi-macos`, `crates/wifi-windows`, `app/` — Claude Code
- [ ] Scaffold Tauri v2 app with tray icon and empty Svelte settings window — Claude Code
- [ ] `ci.yml`: fmt, clippy, tests, build on Linux, macOS, Windows — Claude Code
- [ ] Create menu bar template icons (0–3 arcs, disconnected) from the small SVG — Claude Code, you review
- [ ] Turn on branch protection for `main` and Dependabot — you

## Phase 1 – Core engine (weeks 2–3)

Done when: the engine passes scripted tests for every scenario in the PRD, with no OS code.

- [ ] Types: `Link`, `ScanResult`, `Sample`, error enum, `WifiBackend` trait — Claude Code
- [ ] `FakeBackend` for tests and `--simulate` — Claude Code
- [ ] Engine states (Watching, Scanning, Switching, Backoff, Paused) and `step()` — Claude Code
- [ ] Rules: weak signal, disconnect, 15 s persistence, 8 dBm margin, priority tie-break — Claude Code
- [ ] Backoff 30 s → 5 min and reset on recovery — Claude Code
- [ ] Config load/save (TOML), defaults, migration, corrupt-file handling — Claude Code
- [ ] Scenario tests: walk away, flapping signal, two equal networks, drop and reconnect — Claude Code
- [ ] Review the rules against real dorm behaviour — you

## Phase 2 – Platform backends (weeks 4–6)

Done when: both backends read the link, scan, list known networks and connect on real laptops.

**macOS (weeks 4–5)**

- [ ] CoreWLAN via `objc2-core-wlan`: current link, RSSI, scan, connect — Claude Code
- [ ] Keychain store for passwords (service `ThunderWiFi`) — Claude Code
- [ ] Location permission request and status — Claude Code
- [ ] Gateway lookup; sleep/wake notifications — Claude Code
- [ ] Test on your Mac with the 3 dorm networks — you

**Windows (weeks 5–6)**

- [ ] wlanapi via `windows` crate: open handle, current link, BSS list RSSI — Claude Code
- [ ] Scan with completion notification; connect by saved profile — Claude Code
- [ ] Known networks from `WlanGetProfileList`; gateway lookup — Claude Code
- [ ] Location permission check on Windows 11 24H2+ — Claude Code
- [ ] Test on a real Windows laptop — Windows tester

## Phase 3 – Tray UI and settings (weeks 7–8)

Done when: a user can install, add networks, and watch it switch on signal alone. Release **v0.1 Mac alpha** to a few friends.

- [ ] Tray icon with signal level and optional `-58 · 24ms` label — Claude Code
- [ ] Tray menu: status, last event, auto-switch toggle, Switch to, Settings, Quit — Claude Code
- [ ] Settings window: Networks tab (add, remove, drag order, Mac password field) — Claude Code
- [ ] Settings window: Rules, General, About tabs — Claude Code
- [ ] Wire engine actions to backend on a worker thread; events to UI — Claude Code
- [ ] First-run flow: explain and request Location permission — Claude Code
- [ ] Open at login (autostart plugin) — Claude Code
- [ ] Use it daily for a week and file issues — you

## Phase 4 – Quality checks (weeks 9–10)

Done when: the app switches away from a network with strong signal but bad internet, on both OSes. Release **v0.5 beta** for Mac and Windows.

- [ ] Prober: ICMP to router, 1.1.1.1, 8.8.8.8 every 5 s, 30 s rolling window — Claude Code
- [ ] Unprivileged ICMP on macOS; `IcmpSendEcho2` wrapper on Windows — Claude Code
- [ ] TCP port-443 fallback when ICMP is blocked — Claude Code
- [ ] Engine rules for bad internet; "bad internet" marks for 10 minutes — Claude Code
- [ ] Show ping in tray label and menu — Claude Code
- [ ] Test by unplugging one router's internet while Wi-Fi stays up — you
- [ ] Same test on Windows — Windows tester
- [ ] Check idle CPU under 1% and memory under 50 MB — you, Windows tester

## Phase 5 – Packaging and release (weeks 11–12)

Done when: pushing a version tag produces signed installers on GitHub Releases that update themselves.

- [ ] `release.yml` with `tauri-action`: universal .dmg, .msi for x64 and ARM64 — Claude Code
- [ ] Apply to SignPath for free Windows signing — you
- [ ] Decide on Apple Developer ID ($99/year) or ship unsigned with Homebrew instructions — you
- [ ] Tauri updater: signing key in secrets, `latest.json` on Releases — Claude Code
- [ ] Switch notifications and the 24-hour history graph (nice-to-have) — Claude Code
- [ ] Bahasa Indonesia translation — you
- [ ] `docs/TESTING.md` manual checklist, run once on each OS — you, Windows tester

## Phase 6 – Community and v1.0 launch (weeks 13–14)

Done when: **v1.0** is on GitHub Releases, Homebrew and winget, with docs a stranger can follow.

- [ ] README: screenshots, install steps per OS, FAQ (why Location?) — Claude Code, you
- [ ] Homebrew cask and winget manifest, updated by the release workflow — Claude Code
- [ ] Label 5–10 "good first issue" tasks for new contributors — you
- [ ] Launch post (GitHub, Reddit r/macapps and r/windows, campus groups) — you
- [ ] Fix launch-week bugs; tag v1.0.1 — Claude Code, you
- [ ] Review PRD success metrics after 3 months — you

## How we track work

This doc is the plan; day-to-day work moves to GitHub once Phase 0 is done.

- **Issues:** one issue per task above; Claude Code can create them from this file with the `gh` CLI.
- **Milestones:** `v0.1 Mac alpha`, `v0.5 beta`, `v1.0`.
- **Labels:** `macos`, `windows`, `core`, `ui`, `docs`, `good first issue`, `bug`, `enhancement`.
- **Project board:** columns To do, In progress, Review, Done.
- **Rule:** every change goes through a pull request with CI green, even when you are the only developer.
