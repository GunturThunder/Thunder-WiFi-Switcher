//! Try the Windows backend on real Wi-Fi, without the app.
//!
//! ```text
//! cargo run -p wifi-windows --example probe -- <command>
//!
//!   status            Wi-Fi power, location access, current network, router (default)
//!   permission        open Windows' location settings
//!   scan              networks in range, strongest first
//!   known             networks Windows has saved
//!   connect <name>    join a network with the profile Windows saved for it
//!   ping              ping the router, 1.1.1.1 and 8.8.8.8 (and try port 443)
//!   watch             print the signal every 5 seconds, plus sleep and wake
//! ```

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    probe::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The Windows probe only runs on Windows.");
}

#[cfg(windows)]
mod probe {
    use std::process::ExitCode;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use thunder_core::driver::read_sample;
    use thunder_core::{Error, Permission, Sample, WifiBackend};
    use wifi_windows::{PowerWatcher, WinBackend};

    const USAGE: &str = "\
usage: cargo run -p wifi-windows --example probe -- <command>

  status            Wi-Fi power, location access, current network, router (default)
  permission        open Windows' location settings
  scan              networks in range, strongest first
  known             networks Windows has saved
  connect <name>    join a network with the profile Windows saved for it
  ping              ping the router, 1.1.1.1 and 8.8.8.8 (and try port 443)
  watch             print the signal every 5 seconds, plus sleep and wake";

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let wifi = WinBackend::new();
        let result = match args.as_slice() {
            [] | ["status"] => status(&wifi),
            ["permission"] => {
                wifi.request_permission();
                println!(
                    "Opened Settings → Privacy & security → Location. Turn on \"Let desktop apps access your location\"."
                );
                Ok(())
            }
            ["scan"] => scan(&wifi),
            ["known"] => known(&wifi),
            ["connect", ssid] => connect(&wifi, ssid),
            ["ping"] => ping(&wifi),
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
                    eprintln!("hint: location is off for desktop apps; run `probe permission`");
                }
                ExitCode::FAILURE
            }
        }
    }

    fn status(wifi: &WinBackend) -> Result<(), Error> {
        println!("Wi-Fi:      {}", if wifi.powered() { "on" } else { "off" });
        let location = match wifi.permission() {
            Permission::Granted => "allowed",
            Permission::Denied => "blocked (location is off for desktop apps)",
            Permission::NotDetermined => "not asked yet",
        };
        println!("Location:   {location}");
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
                println!("Connected:  can't tell (location is off for desktop apps)")
            }
            Err(err) => return Err(err),
        }
        match wifi.gateway()? {
            Some(router) => println!("Router:     {router}"),
            None => println!("Router:     none"),
        }
        Ok(())
    }

    fn scan(wifi: &WinBackend) -> Result<(), Error> {
        let current = wifi.current().ok().flatten().map(|link| link.ssid);
        let known = wifi.known_networks().unwrap_or_default();
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
            if known.contains(&network.ssid) {
                notes.push("saved in Windows");
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

    fn known(wifi: &WinBackend) -> Result<(), Error> {
        for name in wifi.known_networks()? {
            println!("{name}");
        }
        Ok(())
    }

    fn connect(wifi: &WinBackend, ssid: &str) -> Result<(), Error> {
        println!("Connecting to {ssid}…");
        let started = Instant::now();
        wifi.connect(ssid)?;
        println!("Connected in {:.1} s.", started.elapsed().as_secs_f64());
        status(wifi)
    }

    fn ping(wifi: &WinBackend) -> Result<(), Error> {
        let mut targets: Vec<(String, std::net::IpAddr)> = Vec::new();
        if let Some(router) = wifi.gateway()? {
            targets.push((format!("router {router}"), router));
        }
        for server in ["1.1.1.1", "8.8.8.8"] {
            targets.push((server.to_string(), server.parse().expect("valid address")));
        }
        println!("Target                 Ping        Port 443");
        for (name, addr) in targets {
            let show = |result: thunder_core::Result<Duration>| match result {
                Ok(rtt) => format!("{} ms", rtt.as_millis()),
                Err(Error::Timeout) => "no answer".to_string(),
                Err(err) => format!("{err}"),
            };
            let icmp = show(wifi.ping(addr, Duration::from_secs(2)));
            let tcp = show(wifi.tcp_probe(addr, Duration::from_secs(2)));
            println!("{name:<22} {icmp:<11} {tcp}");
        }
        Ok(())
    }

    fn watch(wifi: &WinBackend) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();
        let _watcher = PowerWatcher::new(move |event| {
            let _ = tx.send(event);
        })?;
        println!("Printing the signal every 5 seconds. Press Ctrl-C to stop.");
        let started = Instant::now();
        loop {
            let elapsed = started.elapsed().as_secs();
            let line = match read_sample(wifi) {
                Ok(Sample::WifiOff) => "Wi-Fi off".to_string(),
                Ok(Sample::Disconnected) => "not connected".to_string(),
                Ok(Sample::Connected(link, _)) => format!("{:>4} dBm  {}", link.rssi, link.ssid),
                Err(err) => format!("error: {err}"),
            };
            println!("{:02}:{:02}  {line}", elapsed / 60, elapsed % 60);
            thread::sleep(Duration::from_secs(5));
            for event in rx.try_iter() {
                println!("       {event:?}");
            }
        }
    }
}
