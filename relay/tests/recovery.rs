//! The relays as they run: a main relay and a second one that only knows it,
//! each with players; the second is killed and started again, and players
//! and relays have to come back to one network by themselves.
#[path = "../../src/mp_proto.rs"]
#[allow(dead_code)]
mod proto;

use proto::{Msg, PlayerState};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const SECRET: &str = "a secret for the recovery test";

/// A relay process, killed when dropped.
struct Relay(Child);
impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn relay(port: u16, name: &str, hub: Option<u16>) -> Relay {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fableflight-relay"));
    c.env("FFMP_PORT", port.to_string()).env("FFMP_HTTP_PORT", "0").env("FFMP_NAME", name).env("FFMP_PEER_SECRET", SECRET);
    // (Not the real network's main relays.)
    c.env("FFMP_HUB", hub.map_or("none".to_string(), |h| format!("127.0.0.1:{h}")));
    Relay(c.stdout(std::process::Stdio::null()).spawn().expect("relay binary"))
}
fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A player as the game is one: Hello until Welcome, a State 15 times a
/// second, Hello again when the relay has gone quiet for 2 s.
struct Player {
    name: String,
    sock: UdpSocket,
    cookie: Option<u64>,
    seq: u32,
    heard: Instant,
    last_hello: Instant,
    /// the others it has been told of, and when last
    sees: HashMap<String, Instant>,
}
impl Player {
    fn new(name: &str, port: u16) -> Self {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.connect(("127.0.0.1", port)).unwrap();
        sock.set_nonblocking(true).unwrap();
        let t = Instant::now() - Duration::from_secs(10);
        Player { name: name.into(), sock, cookie: None, seq: 0, heard: Instant::now(), last_hello: t, sees: HashMap::new() }
    }
    fn tick(&mut self, i: usize) {
        let now = Instant::now();
        if self.cookie.is_some() && now.duration_since(self.heard) > Duration::from_secs(2) {
            self.cookie = None;
        }
        match self.cookie {
            None if now.duration_since(self.last_hello) > Duration::from_millis(500) => {
                self.last_hello = now;
                let _ = self.sock.send(&Msg::Hello { name: self.name.clone() }.encode());
            }
            Some(c) => {
                self.seq += 1;
                let s = PlayerState { seq: self.seq, pos: [6_372_000.0, i as f64 * 50.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
                let _ = self.sock.send(&Msg::State { cookie: c, state: s }.encode());
            }
            _ => {}
        }
        let mut buf = [0u8; 2048];
        while let Ok(n) = self.sock.recv(&mut buf) {
            match Msg::decode(&buf[..n]) {
                Some(Msg::Welcome { cookie, .. }) => {
                    self.cookie = Some(cookie);
                    self.heard = now;
                }
                Some(Msg::Peers { peers }) => {
                    self.heard = now;
                    for (_, name, _) in peers {
                        self.sees.insert(name, now);
                    }
                }
                _ => {}
            }
        }
    }
    /// It has been told of `other` in the last half second.
    fn sees(&self, other: &str) -> bool {
        self.sees.get(other).is_some_and(|t| t.elapsed() < Duration::from_millis(500))
    }
}

/// Runs everyone until `done` holds (true) or `secs` pass (false).
fn run_until(players: &mut [Player], secs: f64, done: impl Fn(&[Player]) -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f64() < secs {
        for (i, p) in players.iter_mut().enumerate() {
            p.tick(i);
        }
        if done(players) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(66));
    }
    false
}

#[test]
fn a_relay_killed_and_restarted_comes_back_with_its_players() {
    let (hub_port, other_port) = (free_port(), free_port());
    let _hub = relay(hub_port, "Hub", None);
    let other = relay(other_port, "Other", Some(hub_port));
    // Ten players on each relay.
    let mut players: Vec<Player> = (0..10).map(|i| Player::new(&format!("h{i}"), hub_port)).chain((0..10).map(|i| Player::new(&format!("o{i}"), other_port))).collect();
    let all_see_across = |ps: &[Player]| ps[..10].iter().all(|p| p.sees("o9")) && ps[10..].iter().all(|p| p.sees("h9"));

    assert!(run_until(&mut players, 10.0, all_see_across), "the two relays never joined up");

    // The second relay dies: its players vanish from the main relay's within seconds.
    drop(other);
    assert!(run_until(&mut players, 6.0, |ps| ps[..10].iter().all(|p| !p.sees("o9"))), "dead relay's players linger");
    // (The main relay's own players still see each other.)
    assert!(run_until(&mut players, 2.0, |ps| ps[..10].iter().all(|p| p.sees("h0") || p.name == "h0")));

    // It is started again on the same port: its players get back in by
    // themselves, it rejoins the network, and everyone sees across again.
    let _other = relay(other_port, "Other", Some(hub_port));
    assert!(run_until(&mut players, 15.0, all_see_across), "the network did not come back");
}
