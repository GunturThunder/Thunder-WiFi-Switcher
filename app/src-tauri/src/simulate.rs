//! `--simulate`: pretend networks whose signal changes on a loop, so the app
//! can be tried (and worked on) without real Wi-Fi switching.
//!
//! A 6-minute loop: sit at your desk on Dorm-5G, walk to the common room
//! (Dorm-5G fades, Dorm-2.4 gets stronger, so the app switches), walk back,
//! then Dorm-2.4's router restarts for 15 seconds (the app reconnects).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use thunder_core::fake::FakeBackend;

/// The pretend networks, also used as the default network list.
pub const NETWORKS: [&str; 3] = ["Dorm-5G", "Dorm-2.4", "Library"];

/// Seconds per step of the loop.
const STEP: Duration = Duration::from_secs(5);

/// Starts the pretend Wi-Fi and the thread that changes its signal.
pub fn start() -> Arc<FakeBackend> {
    let wifi = Arc::new(FakeBackend::new());
    let (five, two, library) = levels(0);
    wifi.set_network(NETWORKS[0], five);
    wifi.set_network(NETWORKS[1], two);
    wifi.set_network(NETWORKS[2], library);
    wifi.join(NETWORKS[0]);

    let shared = Arc::clone(&wifi);
    thread::Builder::new()
        .name("thunder-simulate".into())
        .spawn(move || {
            for step in (0..72).cycle() {
                thread::sleep(STEP);
                let (five, two, library) = levels(step);
                shared.set_network(NETWORKS[0], five);
                shared.set_network(NETWORKS[2], library);
                // Dorm-2.4's router restarts near the end of the loop.
                if (60..63).contains(&step) {
                    shared.remove_network(NETWORKS[1]);
                } else {
                    shared.set_network(NETWORKS[1], two);
                }
            }
        })
        .expect("can start the simulation thread");
    wifi
}

/// Signal (dBm) of Dorm-5G, Dorm-2.4 and Library at a step of the loop.
fn levels(step: u32) -> (i32, i32, i32) {
    let walk = |from: i32, to: i32, i: u32| from + (to - from) * i as i32 / 11;
    let (five, two) = match step {
        0..=11 => (-55, -72), // at your desk
        12..=23 => (walk(-55, -86, step - 12), walk(-72, -60, step - 12)), // walking away
        24..=35 => (-86, -60), // in the common room
        36..=47 => (walk(-86, -55, step - 36), walk(-60, -72, step - 36)), // walking back
        _ => (-55, -72),      // back at your desk
    };
    (five, two, -88)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_away_makes_dorm_5g_weak_and_dorm_2_4_clearly_better() {
        let (five, two, _) = levels(30);
        assert!(five < -75);
        assert!(two - five >= 8);
    }

    #[test]
    fn the_walk_is_smooth() {
        assert_eq!(levels(12).0, -55);
        assert_eq!(levels(23).0, -86);
        assert_eq!(levels(36).0, -86);
        assert_eq!(levels(47).0, -55);
    }
}
