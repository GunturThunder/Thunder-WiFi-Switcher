# DormWiFi prototype (reference only)

This is the original Swift prototype that Thunder WiFi Switcher grew out of: a single-file menu bar app that switches to a stronger saved Wi-Fi network when the current one stays weak.

**It isn't part of the product.** CI doesn't build it, and it won't get fixes or features. It's kept because it shows working CoreWLAN, CoreLocation, Keychain and `SMAppService` calls, which the Rust `wifi-macos` backend follows. The [technical design](../../docs/TECHNICAL-DESIGN.md#macos-backend) maps each prototype call to the backend.

| File | What it is |
| --- | --- |
| `main.swift` | The whole app: settings, Keychain, scanning, switching, menu |
| `Info.plist` | Bundle settings, including `LSUIElement` and the Location usage text |
| `build.sh` | Compiles the app and installs it to `/Applications` |

Differences from the real app:

- It only switches on signal strength (no ping or packet-loss checks).
- It stores settings in `UserDefaults`, not the TOML config file.
- Its Keychain items use the service name `DormWiFi`; the real app uses `ThunderWiFi`.

## Building it

Needs macOS 14 or later and the Xcode Command Line Tools (`xcode-select --install`).

```bash
./build.sh
```

Note that `build.sh` replaces any existing `/Applications/DormWiFi.app` and launches the new one.
