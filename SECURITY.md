# Security policy

## Supported versions

Thunder WiFi Switcher hasn't had a release yet. Until v1.0, only the latest release (and the `main` branch) gets security fixes.

| Version | Supported |
| --- | --- |
| Latest release | Yes |
| Older releases | No |

## Reporting a vulnerability

**Please don't open a public issue for security problems.**

Report it privately through GitHub instead:

1. Go to the [Security tab](https://github.com/GunturThunder/Thunder-WiFi-Switcher/security) of the repository.
2. Click **Report a vulnerability**.
3. Describe the problem, how to reproduce it, which OS and app version you used, and what an attacker could do with it.

You should get a reply within 7 days. This is a volunteer project, so please be patient, but you won't be ignored. Once a fix is ready we'll agree on a disclosure date with you, and credit you in the release notes unless you'd rather stay anonymous.

## What counts as a security issue

Examples of things we want to hear about privately:

- Wi-Fi passwords leaking out of the macOS Keychain: into logs, the config file, crash reports, or memory longer than needed.
- The app sending data anywhere other than the ping targets and the GitHub update check described in [PRIVACY.md](PRIVACY.md).
- The updater accepting an unsigned or tampered update.
- Anything that lets another app or user on the same computer make Thunder WiFi Switcher join a network, read its settings, or run code.
- The app asking for, or gaining, more privileges than it needs (it should never need admin rights).

Things that are out of scope:

- Weaknesses in the Wi-Fi networks themselves (for example, an open network with no password).
- Captive portals and network sign-in pages, which the app doesn't handle.
- Bugs that only make switching slower or wrong. Please report those as a normal [network report](https://github.com/GunturThunder/Thunder-WiFi-Switcher/issues/new/choose).
