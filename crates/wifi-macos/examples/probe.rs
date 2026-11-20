//! Try the macOS backend on real Wi-Fi, without the app.
//!
//! Run it with `scripts/probe-macos.sh`, which wraps it in a small app bundle:
//! macOS never shows the Location prompt to a plain command-line tool.
//!
//! ```text
//! scripts/probe-macos.sh <command>
//!
//!   status            Wi-Fi power, permission, current network, router (default)
//!   permission        ask for Location permission (shows the macOS prompt)
//!   scan              networks in range, strongest first
//!   known             networks macOS or the app can join
//!   save <name>       save a network's password in the Keychain (asks for it)
//!   forget <name>     delete a saved password
//!   connect <name>    join a network, using its saved password
//!   watch             print the signal every 5 seconds, plus sleep and wake
//! ```

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    probe::main()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The macOS probe only runs on macOS.");
}

#[cfg(target_os = "macos")]
mod probe {
    use std::process::ExitCode;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
    use thunder_core::driver::read_sample;
    use thunder_core::{Error, Permission, Sample, WifiBackend};
    use wifi_macos::{MacBackend, SleepWatcher};

    const USAGE: &str = "\
usage: scripts/probe-macos.sh <command>

  status            Wi-Fi power, permission, current network, router (default)
  permission        ask for Location permission (shows the macOS prompt)
  scan              networks in range, strongest first
  known             networks macOS or the app can join
  save <name>       save a network's password in the Keychain (asks for it)
  forget <name>     delete a saved password
  connect <name>    join a network, using its saved password
  watch             print the signal every 5 seconds, plus sleep and wake";

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let wifi = MacBackend::new();
        let result = match args.as_slice() {
            [] | ["status"] => status(&wifi),
            ["permission"] => permission(&wifi),
            ["scan"] => scan(&wifi),
            ["known"] => known(&wifi),
            ["save", ssid] => save(&wifi, ssid),
            ["forget", ssid] => forget(&wifi, ssid),
            ["connect", ssid] => connect(&wifi, ssid),
            ["watch"] => watch(&wifi),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        };
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                if err == Error::PermissionDenied {
                    eprintln!("hint: run `probe permission` first");
                }
                ExitCode::FAILURE
            }
        }
    }

    /// Runs the main run loop for a while, so Location and sleep/wake
    /// callbacks can arrive.
    fn pump(duration: Duration) {
        // SAFETY: reading an extern static constant.
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, duration.as_secs_f64(), false);
    }

    fn permission_text(permission: Permission) -> &'static str {
        match permission {
            Permission::Granted => "granted",
            Permission::Denied => "denied",
            Permission::NotDetermined => "not asked yet",
        }
    }

    fn status(wifi: &MacBackend) -> Result<(), Error> {
        println!("Wi-Fi:      {}", if wifi.powered() { "on" } else { "off" });
        println!("Location:   {}", permission_text(wifi.permission()));
        match wifi.current() {
            Ok(Some(link)) => {
                println!("Connected:  {}", link.ssid);
                println!("Signal:     {} dBm", link.rssi);
                if let Some(channel) = link.channel {
                    println!("Channel:    {channel}");
                }
                if let Some(bssid) = link.bssid {
                    println!("BSSID:      {bssid}");
                }
            }
            Ok(None) => println!("Connected:  no"),
            Err(Error::PermissionDenied) => {
                println!("Connected:  yes, but macOS hides the name (run `probe permission`)")
            }
            Err(err) => return Err(err),
        }
        match wifi.gateway()? {
            Some(router) => println!("Router:     {router}"),
            None => {
                println!("Router:     none (not connected, or another connection is the default)")
            }
        }
        Ok(())
    }

    fn permission(wifi: &MacBackend) -> Result<(), Error> {
        if wifi.permission() == Permission::NotDetermined {
            println!("Asking for Location permission. Answer the macOS prompt…");
            wifi.request_permission();
            let deadline = Instant::now() + Duration::from_secs(60);
            while wifi.permission() == Permission::NotDetermined && Instant::now() < deadline {
                pump(Duration::from_millis(250));
            }
        }
        let now = wifi.permission();
        println!("Location: {}", permission_text(now));
        if now == Permission::Denied {
            println!(
                "Turn it on in System Settings → Privacy & Security → Location Services → Thunder WiFi probe."
            );
        }
        Ok(())
    }

    fn scan(wifi: &MacBackend) -> Result<(), Error> {
        let current = wifi.current().ok().flatten().map(|link| link.ssid);
        let saved = wifi.passwords().saved_networks()?;
        let mut networks = wifi.scan()?;
        networks.sort_by_key(|network| std::cmp::Reverse(network.rssi));
        println!("Signal  Channel  Network");
        for network in networks {
            let name = if network.ssid.is_empty() {
                "(hidden)"
            } else {
                &network.ssid
            };
            let mut notes = Vec::new();
            if current.as_deref() == Some(network.ssid.as_str()) {
                notes.push("connected");
            }
            if saved.contains(&network.ssid) {
                notes.push("password saved");
            }
            let channel = network.channel.map(|c| c.to_string()).unwrap_or_default();
            let notes = if notes.is_empty() {
                String::new()
            } else {
                format!("  ({})", notes.join(", "))
            };
            println!("{:>4}    {:>4}     {name}{notes}", network.rssi, channel);
        }
        Ok(())
    }

    fn known(wifi: &MacBackend) -> Result<(), Error> {
        let saved = wifi.passwords().saved_networks()?;
        for name in wifi.known_networks()? {
            let note = if saved.contains(&name) {
                "  (password saved in the app)"
            } else {
                ""
            };
            println!("{name}{note}");
        }
        Ok(())
    }

    fn save(wifi: &MacBackend, ssid: &str) -> Result<(), Error> {
        let password = rpassword::prompt_password(format!(
            "Password for {ssid} (leave empty for an open network): "
        ))
        .map_err(|err| Error::Os(err.to_string()))?;
        wifi.passwords().set(ssid, &password)?;
        if password.is_empty() {
            println!("No password saved for {ssid} (open network).");
        } else {
            println!("Saved the password for {ssid} in the Keychain (service ThunderWiFi).");
        }
        Ok(())
    }

    fn forget(wifi: &MacBackend, ssid: &str) -> Result<(), Error> {
        wifi.passwords().delete(ssid)?;
        println!("Deleted any saved password for {ssid}.");
        Ok(())
    }

    fn connect(wifi: &MacBackend, ssid: &str) -> Result<(), Error> {
        if !wifi.passwords().contains(ssid)? {
            println!(
                "No password saved for {ssid}; trying without one. Use `probe save {ssid}` first if it has a password."
            );
        }
        println!("Connecting to {ssid}…");
        let started = Instant::now();
        wifi.connect(ssid)?;
        println!("Connected in {:.1} s.", started.elapsed().as_secs_f64());
        status(wifi)
    }

    fn watch(wifi: &MacBackend) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();
        let _watcher = SleepWatcher::new(move |event| {
            let _ = tx.send(event);
        });
        println!("Printing the signal every 5 seconds. Press Ctrl-C to stop.");
        let started = Instant::now();
        loop {
            let elapsed = started.elapsed().as_secs();
            let line = match read_sample(wifi) {
                Ok(Sample::WifiOff) => "Wi-Fi off".to_string(),
                Ok(Sample::Disconnected) => "not connected".to_string(),
                Ok(Sample::Connected(link)) => format!("{:>4} dBm  {}", link.rssi, link.ssid),
                Err(err) => format!("error: {err}"),
            };
            println!("{:02}:{:02}  {line}", elapsed / 60, elapsed % 60);
            pump(Duration::from_secs(5));
            for event in rx.try_iter() {
                println!("       {event:?}");
            }
        }
    }
}
