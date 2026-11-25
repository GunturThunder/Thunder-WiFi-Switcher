<p align="center">
  <img src="assets/icons/thunder-wifi-macos.svg" width="128" height="128" alt="Thunder WiFi Switcher icon">
</p>

<h1 align="center">Thunder WiFi Switcher</h1>

<p align="center">
  The free, open-source app for Mac and Windows that keeps you on the network with the best internet, not just the strongest signal.
</p>

<p align="center">
  <a href="https://github.com/GunturThunder/Thunder-WiFi-Switcher/releases/latest"><strong>Download for Mac and Windows</strong></a>
  · <a href="#install">How to install</a>
</p>

<p align="center">
  <a href="https://github.com/GunturThunder/Thunder-WiFi-Switcher/releases/latest"><img src="https://img.shields.io/github/v/release/GunturThunder/Thunder-WiFi-Switcher?label=download" alt="Latest release"></a>
  <a href="https://github.com/GunturThunder/Thunder-WiFi-Switcher/actions/workflows/ci.yml"><img src="https://github.com/GunturThunder/Thunder-WiFi-Switcher/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Windows-lightgrey.svg" alt="macOS and Windows">
</p>

> [!NOTE]
> **Early days.** Versions before 1.0 are alpha and beta builds for testers: expect rough edges, and please tell us about them with a [network report](https://github.com/GunturThunder/Thunder-WiFi-Switcher/issues/new?template=network-report.yml).

## Why

Laptops stay on the Wi-Fi network they joined, even after it turns weak or overloaded. macOS won't move to a stronger saved network while it is still connected, and Windows only roams between access points that share one network name. In a dorm or shared house with several networks, that means slow pages, buffering and dropped calls while a better network sits in range.

Signal strength alone isn't enough either: a network can show full bars and still have high ping, or no internet at all, when everyone is on it.

## What it does

(The full plan for v1.0 is in the [PRD](docs/PRD.md).)

- **Switch on weak signal:** move to a clearly stronger saved network when the signal stays weak.
- **Switch on bad internet:** move when ping or packet loss stays bad, even with a strong signal.
- **Reconnect when dropped:** join the strongest saved network when Wi-Fi disconnects.
- **Menu bar / tray status:** signal (dBm) and ping next to the Wi-Fi icon.
- **Your networks, your order:** pick which networks to use and drag them into preferred order.
- **Places:** group networks by where you use them (Dorm, Office); the menu shows only the place you're in.
- **Join now, with clear errors:** if joining fails, it says why (not in range, wrong password, …) and offers the fix.
- **Setup guide:** explains each permission before macOS or Windows asks for it.
- **Switch notifications** (optional): says where it moved and why.
- **Automatic updates:** new versions download in the background; you choose when to restart.
- **Pause and thresholds:** turn auto-switch off, or set your own weak-signal level and ping and packet-loss limits.
- **Battery friendly:** backs off retries, pauses during sleep, aims for under 1% CPU.
- **Open at login.**

## How it decides

It checks signal and ping every 5 seconds and only acts when a problem lasts 15 seconds **and** another saved network is clearly better, so it never flips back and forth.

| Setting | Default |
| --- | --- |
| Weak signal | Below -75 dBm (adjustable, -65 to -80) |
| Bad internet | Ping over 300 ms, or packet loss over 10% (at least 2 lost checks), to **both** 1.1.1.1 and 8.8.8.8 |
| Problem must last | 15 seconds (3 checks) |
| Just joined a network | Internet checks wait 10 seconds |
| "Clearly better" | Weak signal: at least 8 dBm stronger. Bad internet: any other network with a usable signal, and the bad one is skipped for 10 minutes |
| Nothing better in range | Retry after 30 s, doubling up to 5 minutes |

The router is pinged separately, so it can tell a weak Wi-Fi link from a dead internet connection. If a network blocks ping, it falls back to a normal web connection test (port 443).

It only ever switches to saved networks in range. Places don't change that: walking from the dorm to the office next door still moves you over.

## How it compares

| Tool | Platforms | Price / license | Switches on |
| --- | --- | --- | --- |
| **Thunder WiFi Switcher** | macOS + Windows | Free, open source (MIT) | Signal + ping + packet loss |
| [Wifinian](https://github.com/emoacht/wifinian) | Windows only | Free, open source (MIT) | Signal strength |
| [Wifiry](https://apps.apple.com/app/id1177934624) | macOS only | Paid, closed source | Signal strength |
| [Switchy](https://github.com/devdil/switchy) and personal scripts | macOS only | Free | Failover; many relied on the `airport` command Apple removed in macOS 14.4 |

## Platforms

| | macOS | Windows |
| --- | --- | --- |
| Minimum version | macOS 14 Sonoma | Windows 10 22H2, Windows 11 |
| Hardware | Apple Silicon and Intel | x64 and ARM64 |
| Permission | Location (to read network names) | Location, on Windows 11 24H2 and later |

## Privacy

Both macOS and Windows hide Wi-Fi network names from apps that don't have Location permission, so the app asks for it. It never reads, stores or sends your location. The only traffic it creates is a tiny ping to your router, 1.1.1.1 and 8.8.8.8, plus an update check to GitHub. No analytics, no telemetry. Details in [PRIVACY.md](PRIVACY.md).

## Install

Download the latest installer from [Releases](https://github.com/GunturThunder/Thunder-WiFi-Switcher/releases):

- **macOS 14 or later** (Apple Silicon and Intel): the `.dmg`. Open it and drag Thunder WiFi Switcher to Applications.
- **Windows 10 22H2 or Windows 11:** the `x64` `.msi`, or the `arm64` one on ARM laptops.

The builds aren't signed by Apple or Microsoft, so the first launch needs one extra step (Windows signing is planned):

- **macOS:** open the app once. When macOS says it can't check it, go to **System Settings → Privacy & Security** and click **Open Anyway**. Or run this in Terminal: `xattr -dr com.apple.quarantine "/Applications/Thunder WiFi Switcher.app"`.
- **Windows:** if SmartScreen says "Windows protected your PC", click **More info**, then **Run anyway**.

When the app first starts, its setup guide walks you through it: allow Location access (macOS), or turn on **Let desktop apps access your location** (Windows 11 24H2 and later). Both systems hide Wi-Fi network names from apps without it; [PRIVACY.md](PRIVACY.md) explains why that's safe.

**Updates** come by themselves: the app downloads a new version in the background, and **Restart to Update** in its menu installs it. On macOS, allow Location again after an update (the setup guide opens to help).

## Build from source

You need Rust and Node.js; see [CONTRIBUTING.md](CONTRIBUTING.md) for the full setup on each OS.

```bash
scripts/run-app-macos.sh              # real Wi-Fi (macOS)
scripts/run-app-macos.sh --simulate   # pretend networks
```

## Roadmap

| Release | What's in it |
| --- | --- |
| v0.1 Mac alpha | Menu bar app that switches on signal strength (macOS) |
| v0.5 beta | Adds internet-quality checks, Windows, notifications and auto-update |
| v1.0 | Signed Windows installer, Homebrew tap and winget |

See [docs/ROADMAP.md](docs/ROADMAP.md) for the phase-by-phase plan and [docs/TECHNICAL-DESIGN.md](docs/TECHNICAL-DESIGN.md) for how it's built.

## Contributing

Bug reports, network reports from your dorm or kos, and pull requests are all welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md), and please follow the [Code of Conduct](CODE_OF_CONDUCT.md). To report a security issue, see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
