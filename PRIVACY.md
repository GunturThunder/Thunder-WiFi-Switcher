# Privacy

Thunder WiFi Switcher runs entirely on your computer. It has no account, no analytics and no telemetry, and it never sends anything about you to the developers or anyone else.

## Why the app asks for Location permission

The app needs to see the names (SSIDs) of the Wi-Fi networks around you, so it can tell which of your saved networks are in range. Both operating systems treat Wi-Fi network names as location data, because a list of nearby networks can be used to work out where you are. So they only give network names to apps that have Location permission:

- **macOS:** without Location permission, the Wi-Fi framework (CoreWLAN) returns no network names at all.
- **Windows 11 24H2 and later:** Windows hides network names from apps unless Location is turned on for them. Earlier Windows versions don't ask.

**The app never reads your location.** It doesn't call any location or GPS API, doesn't look up your position, and doesn't store or send anything about where you are. The permission is only there so the operating system will tell it the names of nearby networks.

### Turning the permission off

You can take the permission back at any time. Auto-switching stops working until you turn it on again, because the app can no longer see network names.

- **macOS:** System Settings → Privacy & Security → Location Services → Thunder WiFi Switcher.
- **Windows:** Settings → Privacy & security → Location.

## What the app stores on your computer

| What | Where | Notes |
| --- | --- | --- |
| Settings: your network list and order, thresholds, preferences | macOS: `~/Library/Application Support/ThunderWiFi/config.toml`<br>Windows: `%APPDATA%\ThunderWiFi\config.toml` | Plain text. Contains network names, never passwords. |
| Wi-Fi passwords (macOS only) | macOS Keychain, under the name `ThunderWiFi` | macOS doesn't let apps use the system's saved Wi-Fi passwords, so you enter each one once. They stay in the Keychain and are only read at the moment of connecting. |
| Wi-Fi passwords (Windows) | Not stored by the app | The app uses the network profiles Windows has already saved. |
| Logs | macOS: `~/Library/Logs/ThunderWiFi/`<br>Windows: `%LOCALAPPDATA%\ThunderWiFi\logs\` | Network names, signal strength and ping only. No passwords. No IP addresses except your router's. Rotated at 5 MB. |
| Signal history (planned) | Next to the settings file | For the 24-hour graph. Kept for 7 days. |

Uninstalling the app and deleting these folders (and the `ThunderWiFi` Keychain items on macOS) removes everything it stored.

## What the app sends over the network

| Traffic | To | Why |
| --- | --- | --- |
| Tiny ping (ICMP), or a plain TCP connection to port 443 if ping is blocked, every 5 seconds | Your router, `1.1.1.1` (Cloudflare) and `8.8.8.8` (Google), or other servers you choose in Settings | To measure whether the current network's internet is working. Roughly 0.1 KB per second. The servers see the same thing as any ping: your public IP address. |
| Update check | GitHub (`github.com`) | To see whether a new version is available. Planned for a later release. |

Nothing else. The app doesn't contact any server run by the developers, because there isn't one.

## Changes to this policy

If the app ever needs to store or send something new, this file will be updated in the same pull request, and the change will be listed in the [changelog](CHANGELOG.md).

## Questions

Open an issue on [GitHub](https://github.com/GunturThunder/Thunder-WiFi-Switcher/issues). For anything sensitive, see [SECURITY.md](SECURITY.md).
