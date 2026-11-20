// DormWiFi — a tiny menu bar app that switches to a stronger saved Wi-Fi
// network when the current one gets weak.
//
// Build: ./build.sh   (needs Xcode Command Line Tools: xcode-select --install)

import AppKit
import CoreLocation
import CoreWLAN
import Security
import ServiceManagement

// MARK: - Tunables

let checkInterval: TimeInterval = 5       // how often to read the signal
let weakChecksBeforeSwitch = 3            // 3 x 5 s = signal must be weak for ~15 s
let minImprovement = 8                    // other network must be at least 8 dB stronger
let cooldownAfterAttempt: TimeInterval = 30

// MARK: - Settings (stored in UserDefaults)

enum Settings {
    static let d = UserDefaults.standard

    static var networks: [String] {
        get { d.stringArray(forKey: "networks") ?? [] }
        set { d.set(newValue, forKey: "networks") }
    }
    static var threshold: Int {
        get { d.object(forKey: "threshold") as? Int ?? -75 }
        set { d.set(newValue, forKey: "threshold") }
    }
    static var autoSwitch: Bool {
        get { d.object(forKey: "autoSwitch") as? Bool ?? true }
        set { d.set(newValue, forKey: "autoSwitch") }
    }
}

// MARK: - Keychain (Wi-Fi passwords)

enum Keychain {
    static let service = "DormWiFi"

    private static func base(_ ssid: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: ssid]
    }

    static func save(_ password: String, for ssid: String) {
        SecItemDelete(base(ssid) as CFDictionary)
        guard !password.isEmpty else { return }   // empty = open network
        var add = base(ssid)
        add[kSecValueData as String] = Data(password.utf8)
        SecItemAdd(add as CFDictionary, nil)
    }

    static func load(for ssid: String) -> String? {
        var q = base(ssid)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(q as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    static func delete(for ssid: String) {
        SecItemDelete(base(ssid) as CFDictionary)
    }
}

// MARK: - App

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, CLLocationManagerDelegate {
    let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    let location = CLLocationManager()
    let wifi = CWWiFiClient.shared()
    let work = DispatchQueue(label: "dormwifi.work")

    var timer: Timer?
    var weakCount = 0
    var busy = false
    var lastAttempt = Date.distantPast
    var lastEvent = "Starting…"
    var lastScan: [String: Int] = [:]      // ssid -> strongest RSSI seen in last scan

    var locationGranted: Bool {
        let s = location.authorizationStatus
        return s == .authorizedAlways || s.rawValue == 4   // 4 = authorizedWhenInUse
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let menu = NSMenu()
        menu.delegate = self
        statusItem.menu = menu

        location.delegate = self
        location.requestWhenInUseAuthorization()

        tick()
        timer = Timer.scheduledTimer(withTimeInterval: checkInterval, repeats: true) { [weak self] _ in
            self?.tick()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        DispatchQueue.main.async { self.tick() }
    }

    // MARK: Monitoring

    func tick() {
        guard let iface = wifi.interface() else { updateIcon(nil); return }
        guard iface.powerOn() else {
            weakCount = 0
            lastEvent = "Wi-Fi is turned off"
            updateIcon(nil)
            return
        }

        let ssid = iface.ssid()
        let rssi = iface.rssiValue()
        let connected = rssi != 0
        updateIcon(connected ? rssi : nil)

        guard Settings.autoSwitch, !busy,
              Date().timeIntervalSince(lastAttempt) > cooldownAfterAttempt else { return }
        let managed = Set(Settings.networks)
        guard !managed.isEmpty else { return }

        if !connected {
            // Dropped off everything: join the strongest saved network in range.
            weakCount = 0
            attemptSwitch(allowed: managed, mustBeat: Int.min, current: nil)
            return
        }

        guard let current = ssid, managed.contains(current) else { weakCount = 0; return }

        weakCount = rssi < Settings.threshold ? weakCount + 1 : 0
        if weakCount >= weakChecksBeforeSwitch {
            weakCount = 0
            attemptSwitch(allowed: managed.subtracting([current]),
                          mustBeat: rssi + minImprovement,
                          current: current)
        }
    }

    /// Scans, picks the strongest network in `allowed`, and joins it if its
    /// signal is at least `mustBeat` dBm. Runs off the main thread.
    func attemptSwitch(allowed: Set<String>, mustBeat: Int, current: String?) {
        guard let iface = wifi.interface(), !allowed.isEmpty else { return }
        busy = true
        lastAttempt = Date()
        lastEvent = "Scanning…"

        work.async { [weak self] in
            guard let self else { return }
            var message: String
            var seen: [String: Int] = [:]
            do {
                let found = try iface.scanForNetworks(withSSID: nil)
                for n in found {
                    if let s = n.ssid { seen[s] = max(seen[s] ?? Int.min, n.rssiValue) }
                }
                let candidates = found.filter { n in
                    guard let s = n.ssid else { return false }
                    return allowed.contains(s)
                }
                if let target = candidates.max(by: { $0.rssiValue < $1.rssiValue }),
                   target.rssiValue >= mustBeat,
                   let name = target.ssid {
                    try iface.associate(to: target, password: Keychain.load(for: name))
                    message = "Switched to \(name) (\(target.rssiValue) dBm)"
                } else if current == nil && mustBeat == Int.min && allowed.count == 1 {
                    message = "\(allowed.first!) not in range"
                } else if current == nil {
                    message = "No saved network in range"
                } else {
                    message = "Signal weak, but no better network found"
                }
            } catch {
                message = "Switch failed: \(error.localizedDescription)"
            }

            DispatchQueue.main.async {
                let time = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .short)
                self.lastEvent = "\(time) – \(message)"
                if !seen.isEmpty { self.lastScan = seen }
                self.busy = false
                self.tick()
            }
        }
    }

    // MARK: Menu bar icon

    func updateIcon(_ rssi: Int?) {
        guard let button = statusItem.button else { return }
        let level: Double = rssi.map { min(max(Double($0 + 90) / 40.0, 0.05), 1.0) } ?? 0
        let image = NSImage(systemSymbolName: rssi == nil ? "wifi.slash" : "wifi",
                            variableValue: level,
                            accessibilityDescription: "Wi-Fi")
        image?.isTemplate = true
        button.image = image
        button.imagePosition = .imageLeading
        button.title = rssi.map { " \($0)" } ?? ""
    }

    // MARK: Menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let iface = wifi.interface()
        let ssid = iface?.ssid()
        let rssi = iface?.rssiValue() ?? 0

        if !locationGranted {
            menu.addItem(item("⚠️ Allow Location Access (needed to see network names)…",
                              #selector(openLocationSettings)))
            menu.addItem(.separator())
        }

        let status: String
        if let ssid { status = "Connected: \(ssid)  (\(rssi) dBm)" }
        else if rssi != 0 { status = "Connected (name hidden – allow Location)" }
        else { status = "Not connected" }
        menu.addItem(disabled(status))
        menu.addItem(disabled(lastEvent))
        menu.addItem(.separator())

        let auto = item("Auto-Switch", #selector(toggleAuto))
        auto.state = Settings.autoSwitch ? .on : .off
        menu.addItem(auto)

        let thresholdItem = NSMenuItem(title: "Switch When Weaker Than", action: nil, keyEquivalent: "")
        let thresholdMenu = NSMenu()
        for value in [-65, -70, -75, -80] {
            let label = "\(value) dBm" + (value == -75 ? "  (recommended)" : "")
            let t = item(label, #selector(setThreshold(_:)))
            t.representedObject = value
            t.state = Settings.threshold == value ? .on : .off
            thresholdMenu.addItem(t)
        }
        thresholdItem.submenu = thresholdMenu
        menu.addItem(thresholdItem)
        menu.addItem(.separator())

        menu.addItem(disabled("Your Networks"))
        if Settings.networks.isEmpty {
            menu.addItem(disabled("   (none yet – add your dorm networks below)"))
        }
        for name in Settings.networks {
            var title = "   " + name
            if let r = lastScan[name] { title += "  (\(r) dBm)" }
            let n = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            n.state = (name == ssid) ? .on : .off
            let sub = NSMenu()
            for (label, sel) in [("Switch Now", #selector(switchNow(_:))),
                                 ("Change Password…", #selector(changePassword(_:))),
                                 ("Remove", #selector(removeNetwork(_:)))] {
                let s = item(label, sel)
                s.representedObject = name
                sub.addItem(s)
            }
            n.submenu = sub
            menu.addItem(n)
        }

        let addCurrent = item("Add Current Network…", #selector(addCurrentNetwork))
        if ssid == nil || Settings.networks.contains(ssid!) { addCurrent.action = nil }
        menu.addItem(addCurrent)
        menu.addItem(item("Add Network by Name…", #selector(addNetworkByName)))
        menu.addItem(item("Scan Now", #selector(scanNow)))
        menu.addItem(.separator())

        let login = item("Open at Login", #selector(toggleLogin))
        login.state = SMAppService.mainApp.status == .enabled ? .on : .off
        menu.addItem(login)
        menu.addItem(item("Quit DormWiFi", #selector(quit)))
    }

    func item(_ title: String, _ action: Selector) -> NSMenuItem {
        let i = NSMenuItem(title: title, action: action, keyEquivalent: "")
        i.target = self
        return i
    }

    func disabled(_ title: String) -> NSMenuItem {
        let i = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        i.isEnabled = false
        return i
    }

    // MARK: Actions

    @objc func toggleAuto() {
        Settings.autoSwitch.toggle()
        weakCount = 0
    }

    @objc func setThreshold(_ sender: NSMenuItem) {
        if let v = sender.representedObject as? Int { Settings.threshold = v }
    }

    @objc func switchNow(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String, !busy else { return }
        attemptSwitch(allowed: [name], mustBeat: Int.min, current: nil)
    }

    @objc func scanNow() {
        guard !busy, let iface = wifi.interface() else { return }
        busy = true
        work.async { [weak self] in
            let found = (try? iface.scanForNetworks(withSSID: nil)) ?? []
            var seen: [String: Int] = [:]
            for n in found { if let s = n.ssid { seen[s] = max(seen[s] ?? Int.min, n.rssiValue) } }
            DispatchQueue.main.async {
                self?.lastScan = seen
                self?.lastEvent = "Scan found \(seen.count) networks"
                self?.busy = false
            }
        }
    }

    @objc func changePassword(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String,
              let pw = askPassword(for: name) else { return }
        Keychain.save(pw, for: name)
    }

    @objc func removeNetwork(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        Settings.networks.removeAll { $0 == name }
        Keychain.delete(for: name)
    }

    @objc func addCurrentNetwork() {
        guard let name = wifi.interface()?.ssid(),
              let pw = askPassword(for: name) else { return }
        addNetwork(name, password: pw)
    }

    @objc func addNetworkByName() {
        let alert = NSAlert()
        alert.messageText = "Add a Wi-Fi network"
        alert.informativeText = "Type the exact network name (case-sensitive) and its password. Leave the password empty for an open network."
        let nameField = NSTextField(frame: NSRect(x: 0, y: 30, width: 260, height: 24))
        nameField.placeholderString = "Network name"
        let pwField = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        pwField.placeholderString = "Password"
        let box = NSView(frame: NSRect(x: 0, y: 0, width: 260, height: 54))
        box.addSubview(nameField)
        box.addSubview(pwField)
        alert.accessoryView = box
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        NSApp.activate()
        alert.window.initialFirstResponder = nameField
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let name = nameField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        addNetwork(name, password: pwField.stringValue)
    }

    func addNetwork(_ name: String, password: String) {
        if !Settings.networks.contains(name) { Settings.networks.append(name) }
        Keychain.save(password, for: name)
    }

    func askPassword(for ssid: String) -> String? {
        let alert = NSAlert()
        alert.messageText = "Password for “\(ssid)”"
        alert.informativeText = "Saved in your Keychain. Leave empty for an open network."
        let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        alert.accessoryView = field
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        NSApp.activate()
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return field.stringValue
    }

    @objc func openLocationSettings() {
        location.requestWhenInUseAuthorization()
        if let url = URL(string: "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices") {
            NSWorkspace.shared.open(url)
        }
    }

    @objc func toggleLogin() {
        do {
            if SMAppService.mainApp.status == .enabled {
                try SMAppService.mainApp.unregister()
            } else {
                try SMAppService.mainApp.register()
            }
        } catch {
            lastEvent = "Open at Login failed: \(error.localizedDescription)"
        }
    }

    @objc func quit() { NSApp.terminate(nil) }
}

// MARK: - Entry point

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)   // menu bar only, no Dock icon
app.run()
