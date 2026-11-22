# Release testing checklist

Automated tests cover the switching rules with pretend networks. Before each release, run this checklist once on a real Mac and once on a real Windows laptop, using the installer from the draft release (not a development build). It takes about 30 minutes.

You need two of your own Wi-Fi networks in range (for example a router and an extender with different names, or two dorm networks), plus a way to cut one router's internet while its Wi-Fi stays on: unplug its internet cable, or block it in the router's settings.

Copy the checklist into the release's pull request or issue and tick it off. Note anything odd, with the time, and attach the log:
- macOS: `~/Library/Logs/ThunderWiFi/thunderwifi.log`
- Windows: `%LOCALAPPDATA%\ThunderWiFi\logs\thunderwifi.log`

## Install and first run

- [ ] The installer opens and installs (macOS: drag to Applications; Windows: run the `.msi`).
- [ ] The first launch works after the one-time unsigned-app step in the [README](../README.md#install).
- [ ] The icon appears in the menu bar / tray, with no Dock icon (macOS) or taskbar window (Windows).
- [ ] The settings window opens by itself on first run, with the Get started card.
- [ ] **macOS:** **Allow Location** shows the macOS prompt; after **Allow**, the card disappears within about 5 seconds and the top of the window shows your network.
- [ ] **Windows 11 24H2+:** with location off for desktop apps, the card says so and **Open Location settings** opens the right page; after turning it on, the network appears.

## Networks

- [ ] **Nearby** lists the networks in range, strongest first.
- [ ] Adding both of your networks works. **macOS:** the password field appears and saving says so (the Keychain may ask once; choose **Always Allow**). **Windows:** the note about saved Windows passwords is shown instead.
- [ ] Dragging (or ↑ ↓) changes the order, and the order is still there after quitting and reopening.
- [ ] **Switch to** in the menu moves to the other network within a few seconds; the menu's second line says "Switched to …".
- [ ] **Join now** in the settings window shows "Joining…", then "Joined …". With a wrong password saved (macOS), it says so under the network with **Change password**; a network out of range says it isn't in range.
- [ ] Turning a network off (checkbox) removes it from **Switch to**.

## Switching on its own

- [ ] **Weak signal:** carry the laptop away from the current router until the menu bar shows -76 or weaker. Within about 30 seconds it moves to the other network, if that one is at least 8 dB stronger. (Or set Rules → Weak signal to -65 to make this easier.)
- [ ] **No internet:** cut the current router's internet while its Wi-Fi stays on. Within about 30 seconds the menu shows "no internet", then the app switches; the second line says "no internet on …".
- [ ] **Dropped connection:** disconnect from Wi-Fi (macOS: hold Option, click the Wi-Fi icon, **Disconnect**). Within about 5 seconds it reconnects to your strongest network.
- [ ] **No back-and-forth:** after a switch, it stays put while the new network is fine, even if the old one becomes stronger again.
- [ ] **Auto-switch off:** untick Auto-switch in the menu; repeat the weak-signal test; nothing switches. Tick it again.

## Sleep, Wi-Fi off, restart

- [ ] Sleep the laptop for a few minutes and wake it. The app doesn't fight the system's own reconnect, and the menu shows the right network within about 15 seconds.
- [ ] Turn Wi-Fi off: the menu says "Wi-Fi is off" and the icon is crossed out. Turn it back on: it reconnects.
- [ ] **Open at login** (General tab): turn it on, restart the computer, and the app starts by itself. Turn it off again.

## Resource use (PRD targets)

Leave the app running for 10 minutes on a working network, then check Activity Monitor (macOS) or Task Manager → Details (Windows).

- [ ] CPU while idle stays under 1%.
- [ ] Memory stays under 50 MB with the settings window closed.

## Upgrade and uninstall

- [ ] Installing the new version over the previous one keeps your networks, order and settings.
- [ ] Uninstalling removes the app: drag it to the Bin (macOS), or use Settings → Apps (Windows). The settings and logs folders listed in [PRIVACY.md](../PRIVACY.md) can be deleted by hand.
