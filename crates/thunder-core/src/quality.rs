//! Internet-quality checks: every 5 seconds, probe the router and the public
//! servers once each (in parallel), keep the last 30 seconds per target, and
//! judge whether the network's internet works.
//!
//! - Router slow or not answering: a Wi-Fi link problem ([`Internet::BadLink`]).
//! - Router fine but *every* public server slow or losing packets: an internet
//!   problem ([`Internet::Bad`]). One server's outage never counts.
//! - A target that ignores ping is tried with a normal TCP connection to port
//!   443; after 3 such probes it stays on TCP for that network.
//! - The first 10 seconds on a network aren't checked, and one lost probe in
//!   the window doesn't count as loss: both are normal on working Wi-Fi.
//!
//! All network I/O goes through [`WifiBackend::ping`] and
//! [`WifiBackend::tcp_probe`], so tests use pretend results.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use crate::wifi::{Error, Result, WifiBackend};

/// Results kept per target: 6 probes, 5 seconds apart = 30 seconds.
pub const WINDOW: usize = 6;
/// A target is only judged once it has this many results (15 seconds), so a
/// just-joined network isn't called bad on its first lost probe.
pub const MIN_SAMPLES: usize = 3;
/// How long to wait for the router.
pub const ROUTER_TIMEOUT: Duration = Duration::from_secs(1);
/// How long to wait for a public server.
pub const PUBLIC_TIMEOUT: Duration = Duration::from_secs(2);
/// Ping must fail this many times while TCP works before a target switches
/// to TCP for the rest of the network.
pub const ICMP_MISSES_BEFORE_TCP: u8 = 3;
/// Port for the TCP fallback (HTTPS, open on the default public servers).
pub const TCP_PORT: u16 = 443;
/// Rounds of checks skipped after joining a network or starting (about 10
/// seconds). While a network is still connecting (getting an address, …),
/// pings get lost for reasons that say nothing about the network.
pub const GRACE_ROUNDS: u32 = 2;
/// The odd lost ping is normal on Wi-Fi, so loss only counts once at least
/// this many probes in the window got no answer.
pub const MIN_LOST: u8 = 2;

/// What the latest checks say about the internet behind the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Internet {
    /// Not judged yet (just joined, or checks are paused).
    #[default]
    Unknown,
    /// Works.
    Good,
    /// Every public server is slow or losing packets.
    Bad {
        /// Best median ping among the public servers, if any answered.
        ping_ms: Option<u32>,
        /// Lowest packet loss among the public servers.
        loss_pct: u8,
    },
    /// The router itself is slow or not answering: the Wi-Fi link is bad.
    BadLink {
        /// Router's median ping, if it answered at all.
        ping_ms: Option<u32>,
        /// Router's packet loss.
        loss_pct: u8,
    },
}

/// The last 30 seconds for one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Median round-trip time of the probes that answered.
    pub ping_ms: Option<u32>,
    /// Share of probes that got no answer.
    pub loss_pct: u8,
    /// Probes that got no answer.
    pub lost: u8,
    /// Probes in the window (up to [`WINDOW`]).
    pub samples: u8,
}

impl Stats {
    fn from_results(results: &VecDeque<Option<Duration>>) -> Self {
        let mut answered: Vec<u32> = results
            .iter()
            .flatten()
            .map(|rtt| u32::try_from(rtt.as_millis()).unwrap_or(u32::MAX))
            .collect();
        answered.sort_unstable();
        let samples = results.len();
        let lost = samples - answered.len();
        Self {
            // The upper middle for an even count: errs towards "slow".
            ping_ms: answered.get(answered.len() / 2).copied(),
            loss_pct: (lost * 100).checked_div(samples).unwrap_or(0) as u8,
            lost: lost as u8,
            samples: samples as u8,
        }
    }

    fn judged(&self) -> bool {
        usize::from(self.samples) >= MIN_SAMPLES
    }

    fn is_bad(&self, bad_ping_ms: u32, bad_loss_pct: u32) -> bool {
        let lossy = self.lost >= MIN_LOST && u32::from(self.loss_pct) > bad_loss_pct;
        lossy || self.ping_ms.is_some_and(|ms| ms > bad_ping_ms)
    }
}

/// One target's results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetStats {
    /// The target.
    pub addr: IpAddr,
    /// Its last 30 seconds.
    pub stats: Stats,
    /// Whether it has answered at least once on this network. A router that
    /// never answers (it ignores ping and has no port 443) can't be judged.
    pub answered: bool,
    /// Whether it's being checked over TCP because it ignores ping.
    pub tcp: bool,
}

/// Results of the latest round of probes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Quality {
    /// The router, if its address is known.
    pub router: Option<TargetStats>,
    /// The public servers, in settings order.
    pub public: Vec<TargetStats>,
}

impl Quality {
    /// The best median ping among public servers, for the menu bar.
    pub fn ping_ms(&self) -> Option<u32> {
        self.public.iter().filter_map(|t| t.stats.ping_ms).min()
    }

    /// The lowest packet loss among public servers.
    pub fn loss_pct(&self) -> Option<u8> {
        self.public.iter().map(|t| t.stats.loss_pct).min()
    }

    /// Each target's results in plain words, for the log. For example:
    /// `router 192.168.1.1: 0 of 6 lost, 4 ms · 1.1.1.1: 2 of 6 lost, 41 ms`.
    pub fn describe(&self) -> String {
        self.router
            .iter()
            .map(|target| (format!("router {}", target.addr), target))
            .chain(
                self.public
                    .iter()
                    .map(|target| (target.addr.to_string(), target)),
            )
            .map(|(name, target)| {
                let stats = &target.stats;
                let ping = stats
                    .ping_ms
                    .map_or_else(|| "no answers".to_owned(), |ms| format!("{ms} ms"));
                let via = if target.tcp { " (port 443)" } else { "" };
                format!(
                    "{name}: {} of {} lost, {ping}{via}",
                    stats.lost, stats.samples
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Judges the latest results against the user's limits.
pub fn verdict(quality: &Quality, bad_ping_ms: u32, bad_loss_pct: u32) -> Internet {
    if let Some(router) = &quality.router
        && router.answered
        && router.stats.judged()
        && router.stats.is_bad(bad_ping_ms, bad_loss_pct)
    {
        return Internet::BadLink {
            ping_ms: router.stats.ping_ms,
            loss_pct: router.stats.loss_pct,
        };
    }
    if quality.public.is_empty() || quality.public.iter().any(|t| !t.stats.judged()) {
        return Internet::Unknown;
    }
    if quality
        .public
        .iter()
        .all(|t| t.stats.is_bad(bad_ping_ms, bad_loss_pct))
    {
        Internet::Bad {
            ping_ms: quality.ping_ms(),
            loss_pct: quality.loss_pct().unwrap_or(100),
        }
    } else {
        Internet::Good
    }
}

#[derive(Debug, Default)]
struct Target {
    results: VecDeque<Option<Duration>>,
    tcp: bool,
    icmp_misses: u8,
    answered: bool,
}

/// What probing one target found.
struct Outcome {
    rtt: Option<Duration>,
    /// Ping failed but the TCP fallback answered.
    icmp_blocked: bool,
    /// Ping answered.
    icmp_ok: bool,
}

/// Keeps the rolling windows. One per app run; it starts over whenever the
/// network changes.
#[derive(Debug, Default)]
pub struct Prober {
    network: Option<String>,
    /// Rounds since the network changed (see [`GRACE_ROUNDS`]).
    rounds: u32,
    targets: HashMap<IpAddr, Target>,
}

impl Prober {
    /// A prober with no history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets all results (disconnected, or checks paused).
    pub fn reset(&mut self) {
        self.network = None;
        self.rounds = 0;
        self.targets.clear();
    }

    /// Probes the router and public servers once, in parallel, and returns
    /// the last 30 seconds for each. Blocks for up to about 4 seconds. The
    /// first [`GRACE_ROUNDS`] rounds on a network probe nothing.
    pub fn probe(
        &mut self,
        backend: &(impl WifiBackend + ?Sized),
        network: &str,
        router: Option<IpAddr>,
        public: &[IpAddr],
    ) -> Quality {
        if self.network.as_deref() != Some(network) {
            self.targets.clear();
            self.rounds = 0;
            self.network = Some(network.to_owned());
        }
        self.rounds = self.rounds.saturating_add(1);

        let mut jobs: Vec<(IpAddr, Duration)> = Vec::new();
        for (addr, timeout) in router
            .map(|addr| (addr, ROUTER_TIMEOUT))
            .into_iter()
            .chain(public.iter().map(|&addr| (addr, PUBLIC_TIMEOUT)))
        {
            if !jobs.iter().any(|(seen, _)| *seen == addr) {
                jobs.push((addr, timeout));
            }
        }
        self.targets
            .retain(|addr, _| jobs.iter().any(|(job, _)| job == addr));

        // While the network may still be connecting, probe nothing (the
        // targets are still listed, with empty windows).
        let outcomes: Vec<(IpAddr, Outcome)> = if self.rounds <= GRACE_ROUNDS {
            for (addr, _) in &jobs {
                self.targets.entry(*addr).or_default();
            }
            Vec::new()
        } else {
            thread::scope(|scope| {
                let handles: Vec<_> = jobs
                    .iter()
                    .map(|&(addr, timeout)| {
                        let tcp = self.targets.get(&addr).is_some_and(|t| t.tcp);
                        (
                            addr,
                            scope.spawn(move || probe_one(backend, addr, timeout, tcp)),
                        )
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|(addr, handle)| {
                        let outcome = handle.join().unwrap_or(Outcome {
                            rtt: None,
                            icmp_blocked: false,
                            icmp_ok: false,
                        });
                        (addr, outcome)
                    })
                    .collect()
            })
        };

        for (addr, outcome) in outcomes {
            let target = self.targets.entry(addr).or_default();
            if target.results.len() == WINDOW {
                target.results.pop_front();
            }
            target.results.push_back(outcome.rtt);
            target.answered |= outcome.rtt.is_some();
            if outcome.icmp_ok {
                target.icmp_misses = 0;
            } else if outcome.icmp_blocked {
                target.icmp_misses = target.icmp_misses.saturating_add(1);
                if target.icmp_misses >= ICMP_MISSES_BEFORE_TCP {
                    target.tcp = true;
                }
            }
        }

        let stats = |addr: IpAddr| {
            let target = &self.targets[&addr];
            TargetStats {
                addr,
                stats: Stats::from_results(&target.results),
                answered: target.answered,
                tcp: target.tcp,
            }
        };
        Quality {
            router: router.map(stats),
            public: public
                .iter()
                .copied()
                .filter(|addr| Some(*addr) != router)
                .map(stats)
                .collect(),
        }
    }
}

fn probe_one(
    backend: &(impl WifiBackend + ?Sized),
    addr: IpAddr,
    timeout: Duration,
    tcp: bool,
) -> Outcome {
    if !tcp && let Ok(rtt) = backend.ping(addr, timeout) {
        return Outcome {
            rtt: Some(rtt),
            icmp_blocked: false,
            icmp_ok: true,
        };
    }
    // Ping blocked or lost (or this target is on TCP): try port 443. If the
    // internet is really down, this fails too and the probe counts as lost.
    match backend.tcp_probe(addr, timeout) {
        Ok(rtt) => Outcome {
            rtt: Some(rtt),
            icmp_blocked: !tcp,
            icmp_ok: false,
        },
        Err(_) => Outcome {
            rtt: None,
            icmp_blocked: false,
            icmp_ok: false,
        },
    }
}

/// TCP connection to port 443, for [`WifiBackend::tcp_probe`]'s default.
pub fn tcp_connect(addr: IpAddr, timeout: Duration) -> Result<Duration> {
    let started = Instant::now();
    match TcpStream::connect_timeout(&SocketAddr::new(addr, TCP_PORT), timeout) {
        Ok(_) => Ok(started.elapsed()),
        // A refusal still proves the host answered.
        Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => Ok(started.elapsed()),
        Err(err) if err.kind() == io::ErrorKind::TimedOut => Err(Error::Timeout),
        Err(err) => Err(Error::Os(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn ms(n: u64) -> Option<Duration> {
        Some(Duration::from_millis(n))
    }

    fn stats(results: &[Option<Duration>]) -> Stats {
        Stats::from_results(&results.iter().copied().collect())
    }

    fn target(addr: &str, stats: Stats, answered: bool) -> TargetStats {
        TargetStats {
            addr: addr.parse().unwrap(),
            stats,
            answered,
            tcp: false,
        }
    }

    fn good() -> Stats {
        stats(&[ms(20), ms(25), ms(22)])
    }

    fn down() -> Stats {
        stats(&[ms(20), None, None])
    }

    fn quality(router: Option<TargetStats>, public: Vec<TargetStats>) -> Quality {
        Quality { router, public }
    }

    #[test]
    fn median_and_loss_over_the_window() {
        let s = stats(&[ms(20), ms(800), None, ms(30), ms(25), None]);
        assert_eq!(s.samples, 6);
        assert_eq!(s.loss_pct, 33);
        assert_eq!(s.ping_ms, Some(30));
        assert_eq!(stats(&[None, None]).ping_ms, None);
        assert_eq!(stats(&[None, None]).loss_pct, 100);
        assert_eq!(stats(&[]).loss_pct, 0);
    }

    #[test]
    fn internet_is_bad_only_when_every_public_server_is() {
        let one_down = quality(
            None,
            vec![
                target("1.1.1.1", down(), true),
                target("8.8.8.8", good(), true),
            ],
        );
        assert_eq!(verdict(&one_down, 300, 10), Internet::Good);

        let both_down = quality(
            None,
            vec![
                target("1.1.1.1", down(), true),
                target("8.8.8.8", down(), true),
            ],
        );
        assert_eq!(
            verdict(&both_down, 300, 10),
            Internet::Bad {
                ping_ms: Some(20),
                loss_pct: 66
            }
        );
    }

    #[test]
    fn slow_counts_as_bad() {
        let slow = stats(&[ms(800), ms(900), ms(20), ms(850)]);
        let both_slow = quality(
            None,
            vec![target("1.1.1.1", slow, true), target("8.8.8.8", slow, true)],
        );
        assert!(matches!(verdict(&both_slow, 300, 10), Internet::Bad { .. }));
        assert_eq!(verdict(&both_slow, 1000, 10), Internet::Good);
    }

    #[test]
    fn needs_three_results_before_judging() {
        let early = stats(&[None, None]);
        let q = quality(
            None,
            vec![
                target("1.1.1.1", early, false),
                target("8.8.8.8", early, false),
            ],
        );
        assert_eq!(verdict(&q, 300, 10), Internet::Unknown);
    }

    #[test]
    fn a_failing_router_is_a_link_problem_but_only_if_it_ever_answered() {
        let public = vec![
            target("1.1.1.1", good(), true),
            target("8.8.8.8", good(), true),
        ];
        let failing = stats(&[ms(3), None, None, None]);

        let answered_before = quality(Some(target("192.168.1.1", failing, true)), public.clone());
        assert_eq!(
            verdict(&answered_before, 300, 10),
            Internet::BadLink {
                ping_ms: Some(3),
                loss_pct: 75
            }
        );

        let never_answered = stats(&[None, None, None]);
        let silent_router = quality(Some(target("192.168.1.1", never_answered, false)), public);
        assert_eq!(verdict(&silent_router, 300, 10), Internet::Good);
    }

    #[test]
    fn one_lost_probe_is_not_loss() {
        // 1 of 6 lost is 16%, over the 10% limit, but the odd lost ping is
        // normal on Wi-Fi.
        let one_lost = stats(&[ms(20), None, ms(22), ms(25), ms(21), ms(23)]);
        assert_eq!(one_lost.lost, 1);
        let q = quality(
            Some(target("192.168.1.1", one_lost, true)),
            vec![
                target("1.1.1.1", one_lost, true),
                target("8.8.8.8", one_lost, true),
            ],
        );
        assert_eq!(verdict(&q, 300, 10), Internet::Good);

        let two_lost = stats(&[ms(20), None, ms(22), None, ms(21), ms(23)]);
        let q = quality(
            None,
            vec![
                target("1.1.1.1", two_lost, true),
                target("8.8.8.8", two_lost, true),
            ],
        );
        assert_eq!(
            verdict(&q, 300, 10),
            Internet::Bad {
                ping_ms: Some(22),
                loss_pct: 33
            }
        );
    }

    #[test]
    fn describe_lists_every_target() {
        let mut on_tcp = target("8.8.8.8", stats(&[None, ms(41), None]), true);
        on_tcp.tcp = true;
        let q = quality(
            Some(target("192.168.1.1", stats(&[ms(4), ms(3)]), true)),
            vec![target("1.1.1.1", stats(&[None, None]), false), on_tcp],
        );
        assert_eq!(
            q.describe(),
            "router 192.168.1.1: 0 of 2 lost, 4 ms · 1.1.1.1: 2 of 2 lost, no answers \
             · 8.8.8.8: 2 of 3 lost, 41 ms (port 443)"
        );
    }

    /// Answers ping or TCP from a fixed script, and counts the probes.
    struct Scripted {
        ping_ok: bool,
        tcp_ok: bool,
        probes: AtomicU32,
    }

    fn scripted(ping_ok: bool, tcp_ok: bool) -> Scripted {
        Scripted {
            ping_ok,
            tcp_ok,
            probes: AtomicU32::new(0),
        }
    }

    impl WifiBackend for Scripted {
        fn current(&self) -> Result<Option<crate::Link>> {
            Ok(None)
        }
        fn scan(&self) -> Result<Vec<crate::ScanResult>> {
            Ok(Vec::new())
        }
        fn connect(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn known_networks(&self) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
        fn gateway(&self) -> Result<Option<IpAddr>> {
            Ok(None)
        }
        fn permission(&self) -> crate::Permission {
            crate::Permission::Granted
        }
        fn request_permission(&self) {}
        fn powered(&self) -> bool {
            true
        }
        fn ping(&self, _: IpAddr, _: Duration) -> Result<Duration> {
            self.probes.fetch_add(1, Ordering::Relaxed);
            if self.ping_ok {
                Ok(Duration::from_millis(20))
            } else {
                Err(Error::Timeout)
            }
        }
        fn tcp_probe(&self, _: IpAddr, _: Duration) -> Result<Duration> {
            self.probes.fetch_add(1, Ordering::Relaxed);
            if self.tcp_ok {
                Ok(Duration::from_millis(30))
            } else {
                Err(Error::Timeout)
            }
        }
    }

    fn targets() -> Vec<IpAddr> {
        vec!["1.1.1.1".parse().unwrap(), "8.8.8.8".parse().unwrap()]
    }

    /// Runs the first rounds on "Dorm", which probe nothing.
    fn past_grace(prober: &mut Prober, backend: &Scripted) {
        for _ in 0..GRACE_ROUNDS {
            prober.probe(backend, "Dorm", None, &targets());
        }
    }

    #[test]
    fn the_first_rounds_on_a_network_probe_nothing() {
        let backend = scripted(false, false);
        let mut prober = Prober::new();
        for _ in 0..2 {
            for round in 1..=GRACE_ROUNDS {
                let q = prober.probe(&backend, "Dorm", None, &targets());
                assert_eq!(backend.probes.load(Ordering::Relaxed), 0, "round {round}");
                assert_eq!(q.public.len(), 2);
                assert_eq!(q.public[0].stats.samples, 0);
                assert_eq!(verdict(&q, 300, 10), Internet::Unknown);
            }
            let q = prober.probe(&backend, "Dorm", None, &targets());
            assert!(backend.probes.load(Ordering::Relaxed) > 0);
            assert_eq!(q.public[0].stats.samples, 1);

            // Starting over (disconnected, paused) waits again.
            prober.reset();
            backend.probes.store(0, Ordering::Relaxed);
        }
    }

    #[test]
    fn blocked_ping_falls_back_to_tcp_then_stays_there() {
        let backend = scripted(false, true);
        let mut prober = Prober::new();
        past_grace(&mut prober, &backend);
        for round in 1..=3 {
            let q = prober.probe(&backend, "Dorm", None, &targets());
            assert_eq!(q.public[0].stats.loss_pct, 0, "round {round}");
            assert_eq!(q.public[0].stats.ping_ms, Some(30));
            assert_eq!(q.public[0].tcp, round >= 3);
        }
        assert_eq!(
            verdict(&prober.probe(&backend, "Dorm", None, &targets()), 300, 10),
            Internet::Good
        );
    }

    #[test]
    fn nothing_answering_is_loss() {
        let backend = scripted(false, false);
        let mut prober = Prober::new();
        past_grace(&mut prober, &backend);
        let mut q = Quality::default();
        for _ in 0..3 {
            q = prober.probe(&backend, "Dorm", None, &targets());
        }
        assert_eq!(q.public[0].stats.loss_pct, 100);
        assert!(!q.public[0].tcp);
        assert!(matches!(
            verdict(&q, 300, 10),
            Internet::Bad {
                ping_ms: None,
                loss_pct: 100
            }
        ));
    }

    #[test]
    fn a_new_network_starts_a_new_window() {
        let backend = scripted(true, true);
        let mut prober = Prober::new();
        for _ in 0..8 {
            prober.probe(&backend, "A", None, &targets());
        }
        let q = prober.probe(&backend, "A", None, &targets());
        assert_eq!(usize::from(q.public[0].stats.samples), WINDOW);
        // B's first rounds are skipped, so its window is empty.
        let q = prober.probe(&backend, "B", None, &targets());
        assert_eq!(q.public[0].stats.samples, 0);
    }

    #[test]
    fn the_router_is_probed_once_even_if_listed_as_a_target() {
        let backend = scripted(true, true);
        let router: IpAddr = "192.168.1.1".parse().unwrap();
        let mut list = targets();
        list.push(router);
        let q = Prober::new().probe(&backend, "A", Some(router), &list);
        assert!(q.router.is_some());
        assert_eq!(q.public.len(), 2);
    }
}
