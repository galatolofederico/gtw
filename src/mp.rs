//! Multiplayer: the other players, through a relay (see `relay/` and the
//! protocol in `mp_proto.rs`). A background thread says Hello, then sends
//! where our aircraft is about fifteen times a second and takes in where the
//! others are. Each of them is drawn a little in the past (`DELAY`), between
//! two reports, so that they move smoothly whatever the network does; past
//! the last report they fly on along their velocity for a moment.
//!
//! On the start screen the connection stays up but nothing of the aircraft
//! is sent: it watches instead (`set_watching`), and the relay sends the
//! whole network's players, coarsely, for the globe (`world`).
//!
//! Nothing is sent unless multiplayer is switched on; what is sent is the
//! aircraft's state and the name the player chose.
use crate::airmodels::{self as am, TYPES};
use crate::mp_proto::{self as proto, Msg, PlayerState, Spot};
use crate::airfield::{Light, L_GREEN, L_RED, L_STROBE, L_WHITE};
use crate::render::Inst;
use glam::{DQuat, DVec3, Vec3};
use std::collections::{HashMap, VecDeque};
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// how far in the past the others are shown (s)
const DELAY: f64 = 0.15;
/// how often our state goes out
const SEND_EVERY: Duration = Duration::from_millis(66);
/// a player not heard of for this long is gone (s)
const STALE: f64 = 3.0;
/// no word from the relay for this long: say Hello again
const LOST: Duration = Duration::from_secs(5);
/// how often a Watch goes out on the start screen
const WATCH_EVERY: Duration = Duration::from_millis(1000);
/// a player on the world map not heard of for this long is gone (s)
const WORLD_STALE: f64 = 3.5;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Connecting,
    Connected,
    Error(String),
}

struct Peer {
    name: String,
    /// reports, oldest first: when they came (s since `epoch`), what they said
    snaps: VecDeque<(f64, PlayerState)>,
}

struct Shared {
    epoch: Instant,
    ours: Mutex<Option<PlayerState>>,
    peers: Mutex<HashMap<u32, Peer>>,
    status: Mutex<Status>,
    stop: AtomicBool,
    /// on the start screen: watch the world, send nothing of the aircraft
    watching: AtomicBool,
    /// the world map: each player as last reported (when, where), the
    /// network's count, and when a World last came (None: never, an older relay)
    world: Mutex<World>,
}

#[derive(Default)]
struct World {
    spots: HashMap<u32, (f64, Spot)>,
    total: u32,
    heard: Option<f64>,
}

/// Another player, where they are now (as best we know).
#[derive(Clone, Debug)]
pub struct Remote {
    pub id: u32,
    pub name: String,
    pub aircraft: u8,
    pub pos: DVec3,
    pub rot: DQuat,
    pub vel: DVec3,
    pub gear: f32,
    pub flags: u8,
    /// the report nearest that instant (controls, flaps, throttle…)
    pub state: PlayerState,
}

pub struct Mp {
    pub server: String,
    pub name: String,
    /// picked by the server list ("auto"), not typed in
    pub auto: bool,
    shared: Arc<Shared>,
}

impl Mp {
    /// Connects to the relay at `server` ("host" or "host:port") as `name`.
    pub fn start(server: &str, name: &str) -> Self {
        let shared = Arc::new(Shared { epoch: Instant::now(), ours: Mutex::new(None), peers: Mutex::new(HashMap::new()), status: Mutex::new(Status::Connecting), stop: AtomicBool::new(false), watching: AtomicBool::new(false), world: Mutex::new(World::default()) });
        let (sv, nm, sh) = (server.trim().to_string(), proto::clean_name(name), shared.clone());
        std::thread::Builder::new()
            .name("multiplayer".into())
            .spawn(move || {
                if let Err(e) = run(&sv, &nm, &sh) {
                    log::warn!("multiplayer: {e}");
                    *sh.status.lock().unwrap() = Status::Error(e);
                }
            })
            .ok();
        Mp { server: server.trim().to_string(), name: proto::clean_name(name), auto: false, shared }
    }

    /// What to send next: our aircraft now.
    pub fn set_state(&self, s: PlayerState) {
        *self.shared.ours.lock().unwrap() = Some(s);
    }

    pub fn status(&self) -> Status {
        self.shared.status.lock().unwrap().clone()
    }

    /// On the start screen (true): our aircraft is no longer sent (the relay
    /// takes us off the players flying) and the world map comes in instead.
    /// In flight (false): the other way round.
    pub fn set_watching(&self, on: bool) {
        if self.shared.watching.swap(on, Ordering::Relaxed) != on {
            if on {
                *self.shared.ours.lock().unwrap() = None;
            }
            // (Nobody near any more, or a world seen from the menu: start afresh either way.)
            self.shared.peers.lock().unwrap().clear();
            let mut w = self.shared.world.lock().unwrap();
            w.spots.clear();
            w.total = 0;
        }
    }

    /// The world map: every player of the network the relay lists, the
    /// number in all (it may list fewer), and whether the relay sends one at
    /// all (false for a relay from before the world map, or not yet).
    pub fn world(&self) -> (Vec<Spot>, u32, bool) {
        let t = self.shared.epoch.elapsed().as_secs_f64();
        let mut w = self.shared.world.lock().unwrap();
        w.spots.retain(|_, (ts, _)| t - *ts < WORLD_STALE);
        let mut out: Vec<Spot> = w.spots.values().map(|(_, s)| s.clone()).collect();
        out.sort_by_key(|s| s.id);
        let known = w.heard.is_some_and(|h| t - h < WORLD_STALE);
        (out, if known { w.total } else { 0 }, known)
    }

    /// The other players, interpolated to now (minus `DELAY`).
    pub fn players(&self) -> Vec<Remote> {
        let t = self.shared.epoch.elapsed().as_secs_f64() - DELAY;
        let peers = self.shared.peers.lock().unwrap();
        let mut out: Vec<Remote> = peers.iter().filter_map(|(id, p)| at(*id, p, t)).collect();
        out.sort_by_key(|r| r.id);
        out
    }
}

impl Drop for Mp {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// A pilot name for someone who has not chosen one: two English words and
/// a number ("SwiftKestrel27"), at most 17 bytes, well under `NAME_MAX`.
pub fn random_name() -> String {
    const ADJ: &[&str] = &["Swift", "Silent", "Brave", "Lucky", "Steady", "Bold", "Quiet", "Rapid", "Sunny", "Stormy", "Misty", "Polar", "Golden", "Silver", "Crimson", "Azure", "Wild", "Calm", "Clever", "Happy", "Noble", "Rusty", "Gentle", "Proud"];
    const NOUN: &[&str] = &["Kestrel", "Falcon", "Heron", "Swallow", "Albatros", "Condor", "Osprey", "Petrel", "Gannet", "Merlin", "Harrier", "Skylark", "Puffin", "Pelican", "Raven", "Swift", "Comet", "Glider", "Zephyr", "Pilot", "Aviator", "Nomad", "Voyager", "Navigator"];
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.write_u32(std::process::id());
    let r = h.finish();
    format!("{}{}{:02}", ADJ[(r % ADJ.len() as u64) as usize], NOUN[((r >> 16) % NOUN.len() as u64) as usize], (r >> 32) % 100)
}

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::new(a[0], a[1], a[2])
}
fn quat(a: [f32; 4]) -> DQuat {
    DQuat::from_xyzw(a[0] as f64, a[1] as f64, a[2] as f64, a[3] as f64).normalize()
}

/// A player at time `t`: between the two reports around it, or carried on
/// from the last one (for at most a second).
fn at(id: u32, p: &Peer, t: f64) -> Option<Remote> {
    let (first, last) = (p.snaps.front()?, p.snaps.back()?);
    let (pos, rot, s) = if t <= first.0 {
        (v3(first.1.pos), quat(first.1.rot), first.1)
    } else if t >= last.0 {
        let dt = (t - last.0).min(1.0);
        (v3(last.1.pos) + v3(last.1.vel.map(|x| x as f64)) * dt, quat(last.1.rot), last.1)
    } else {
        let k = p.snaps.iter().position(|(ts, _)| *ts > t).unwrap_or(p.snaps.len() - 1).max(1);
        let ((t0, a), (t1, b)) = (p.snaps[k - 1], p.snaps[k]);
        let u = ((t - t0) / (t1 - t0).max(1e-6)).clamp(0.0, 1.0);
        (v3(a.pos).lerp(v3(b.pos), u), quat(a.rot).slerp(quat(b.rot), u), if u < 0.5 { a } else { b })
    };
    Some(Remote { id, name: p.name.clone(), aircraft: s.aircraft, pos, rot, vel: v3(s.vel.map(|x| x as f64)), gear: s.gear as f32 / 255.0, flags: s.flags, state: s })
}

/// The network side, until told to stop.
fn run(server: &str, name: &str, sh: &Shared) -> Result<(), String> {
    let with_port = if server.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { server.to_string() } else { format!("{server}:{}", proto::DEFAULT_PORT) };
    let addr = with_port.to_socket_addrs().map_err(|e| format!("{server}: {e}"))?.next().ok_or_else(|| format!("{server}: no address"))?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).map_err(|e| e.to_string())?;
    sock.connect(addr).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_millis(15))).map_err(|e| e.to_string())?;
    let mut cookie: Option<u64> = None;
    let (mut last_send, mut last_hello, mut last_heard) = (Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10), Instant::now());
    let mut last_watch = Instant::now() - Duration::from_secs(10);
    let mut seq = 0u32;
    let mut buf = [0u8; 2048];
    while !sh.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        match cookie {
            None if now.duration_since(last_hello) > Duration::from_secs(1) => {
                last_hello = now;
                let _ = sock.send(&Msg::Hello { name: name.to_string() }.encode());
            }
            Some(c) if sh.watching.load(Ordering::Relaxed) => {
                if now.duration_since(last_watch) >= WATCH_EVERY {
                    last_watch = now;
                    let _ = sock.send(&Msg::Watch { cookie: c }.encode());
                }
            }
            Some(c) if now.duration_since(last_send) >= SEND_EVERY => {
                last_send = now;
                if let Some(mut s) = *sh.ours.lock().unwrap() {
                    seq = seq.wrapping_add(1);
                    s.seq = seq;
                    let _ = sock.send(&Msg::State { cookie: c, state: s }.encode());
                }
            }
            _ => {}
        }
        if cookie.is_some() && now.duration_since(last_heard) > LOST {
            // The relay went quiet (restarted, or the network dropped): start
            // over. (Watching a relay from before the world map, which never
            // answers a Watch, is no reason to look lost.)
            cookie = None;
            let old_relay = sh.watching.load(Ordering::Relaxed) && sh.world.lock().unwrap().heard.is_none();
            if !old_relay {
                *sh.status.lock().unwrap() = Status::Connecting;
            }
        }
        let n = match sock.recv(&mut buf) {
            Ok(n) => n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            // (An ICMP "port unreachable" from a relay that is down: refused
            // on Linux, reset — WSAECONNRESET — on Windows.)
            Err(e) if matches!(e.kind(), std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset) => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        match Msg::decode(&buf[..n]) {
            Some(Msg::Welcome { cookie: c, .. }) => {
                cookie = Some(c);
                last_heard = now;
                last_watch = now - WATCH_EVERY;
                // (Watching, the relay has answered: that is all there is to being connected.)
                if sh.watching.load(Ordering::Relaxed) {
                    *sh.status.lock().unwrap() = Status::Connected;
                }
            }
            Some(Msg::World { total, spots }) => {
                last_heard = now;
                *sh.status.lock().unwrap() = Status::Connected;
                let t = sh.epoch.elapsed().as_secs_f64();
                let mut w = sh.world.lock().unwrap();
                if sh.watching.load(Ordering::Relaxed) {
                    w.total = total;
                    w.heard = Some(t);
                    for s in spots {
                        w.spots.insert(s.id, (t, s));
                    }
                }
            }
            Some(Msg::Peers { peers }) => {
                last_heard = now;
                *sh.status.lock().unwrap() = Status::Connected;
                let t = sh.epoch.elapsed().as_secs_f64();
                let mut map = sh.peers.lock().unwrap();
                for (id, name, s) in peers {
                    let p = map.entry(id).or_insert_with(|| Peer { name: name.clone(), snaps: VecDeque::new() });
                    p.name = name;
                    // (Reordered packets: keep the reports in the order they were sent.)
                    if p.snaps.back().is_none_or(|(_, b)| s.seq > b.seq) {
                        p.snaps.push_back((t, s));
                    }
                    while p.snaps.len() > 8 {
                        p.snaps.pop_front();
                    }
                }
                map.retain(|_, p| p.snaps.back().is_some_and(|(ts, _)| t - ts < STALE));
            }
            _ => {}
        }
    }
    if let Some(c) = cookie {
        let _ = sock.send(&Msg::Bye { cookie: c }.encode());
    }
    Ok(())
}

/// Where the list of the network's servers is read from, tried in turn (a
/// relay's `/servers.json`, see relay/; any relay serves the whole network's).
/// When none answers, the last list received (kept on disk) stands in, and
/// its servers are asked directly. One per main relay (`proto::MAIN_RELAYS`).
pub const DIRECTORY_URLS: &[&str] = &["https://fableflight.danux.be/servers.json", "https://fableflight.rolandiscool.com/servers.json"];

/// A server of the network, as the list gives it and as we hear it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerEntry {
    pub name: String,
    /// "host:port" to connect to
    pub address: String,
    pub up: bool,
    pub flying: u32,
    /// how long it takes to answer us (ms), when it does
    pub ping_ms: Option<u32>,
}

struct DirShared {
    servers: Mutex<Vec<ServerEntry>>,
    error: Mutex<Option<String>>,
    /// the list is the last one received, no list server answering
    stale: AtomicBool,
    now: AtomicBool,
    stop: AtomicBool,
}

/// The network's servers, read every 30 seconds from the main relay and
/// pinged (a Hello and the time to its Welcome), in a background thread.
pub struct Directory {
    shared: Arc<DirShared>,
}

impl Directory {
    /// Reads the list from `urls` (the first that answers), keeping a copy
    /// in `cache` for when none does.
    pub fn start(src: Arc<crate::sources::Sources>, urls: Vec<String>, cache: Option<std::path::PathBuf>) -> Self {
        let shared = Arc::new(DirShared { servers: Mutex::new(Vec::new()), error: Mutex::new(None), stale: AtomicBool::new(false), now: AtomicBool::new(true), stop: AtomicBool::new(false) });
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("server list".into())
            .spawn(move || {
                let mut last = Instant::now() - Duration::from_secs(3600);
                while !sh.stop.load(Ordering::Relaxed) {
                    if sh.now.swap(false, Ordering::Relaxed) || last.elapsed() > Duration::from_secs(30) {
                        last = Instant::now();
                        let (list, error) = read_list(&urls, |u| src.fetch.get(u).map_err(|e| e.to_string()), cache.as_deref());
                        sh.stale.store(list.as_ref().is_some_and(|l| l.1), Ordering::Relaxed);
                        if let Some((list, _)) = list {
                            *sh.servers.lock().unwrap() = list;
                        }
                        *sh.error.lock().unwrap() = error;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
            .ok();
        Directory { shared }
    }
    /// The list is the last one received (no list server answers now).
    pub fn stale(&self) -> bool {
        self.shared.stale.load(Ordering::Relaxed)
    }
    pub fn servers(&self) -> Vec<ServerEntry> {
        self.shared.servers.lock().unwrap().clone()
    }
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().clone()
    }
    /// Reads the list again now.
    pub fn refresh(&self) {
        self.shared.now.store(true, Ordering::Relaxed);
    }
    /// The server that answers us fastest.
    pub fn best(&self) -> Option<ServerEntry> {
        self.servers().into_iter().filter(|s| s.up && s.ping_ms.is_some()).min_by_key(|s| s.ping_ms)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// One reading of the list: from the first of `urls` that answers (then kept
/// in `cache`), else from `cache` (then every server is asked directly, as
/// the list cannot say which are up). Each server that is up is pinged.
/// Returns the list and whether it came from the cache, and the error when
/// no list server answered.
fn read_list(urls: &[String], get: impl Fn(&str) -> Result<Vec<u8>, String>, cache: Option<&std::path::Path>) -> (Option<(Vec<ServerEntry>, bool)>, Option<String>) {
    let mut error = None;
    for u in urls {
        match get(u).and_then(|d| parse_servers(&d, u).ok_or_else(|| crate::i18n::tr("mp.unreadable_list").to_string())) {
            Ok(mut list) => {
                if let Some(c) = cache {
                    let _ = std::fs::write(c, cache_json(&list));
                }
                for s in list.iter_mut().filter(|s| s.up) {
                    s.ping_ms = ping(&s.address);
                }
                return (Some((list, false)), None);
            }
            Err(e) => error = Some(e),
        }
    }
    let cached = cache.and_then(|c| std::fs::read(c).ok()).and_then(|d| parse_servers(&d, "https://cache/"));
    match cached {
        Some(mut list) if !list.is_empty() => {
            for s in list.iter_mut() {
                s.ping_ms = ping(&s.address);
                (s.up, s.flying) = (s.ping_ms.is_some(), 0);
            }
            (Some((list, true)), error)
        }
        _ => (None, error.or_else(|| Some(crate::i18n::tr("mp.no_server_list").into()))),
    }
}

/// The list as kept on disk (the same shape as `/servers.json`, every address given).
fn cache_json(list: &[ServerEntry]) -> String {
    let esc = |s: &str| serde_json::Value::String(s.to_string()).to_string();
    let rows: Vec<String> = list.iter().map(|s| format!("{{\"name\":{},\"address\":{},\"up\":true,\"flying\":0}}", esc(&s.name), esc(&s.address))).collect();
    format!("{{\"servers\":[{}]}}", rows.join(","))
}

/// "host:port", the default port added when there is none.
pub fn with_port(server: &str) -> String {
    let s = server.trim();
    if s.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { s.to_string() } else { format!("{s}:{}", proto::DEFAULT_PORT) }
}

/// The list in `/servers.json`. A server without an address (the main relay
/// itself, when it does not say) is at the list's own host.
fn parse_servers(data: &[u8], url: &str) -> Option<Vec<ServerEntry>> {
    let j: serde_json::Value = serde_json::from_slice(data).ok()?;
    let host = url.split("://").nth(1)?.split(['/', ':']).next()?.to_string();
    let list = j["servers"].as_array()?;
    Some(
        list.iter()
            .take(64)
            .filter_map(|s| {
                let name = proto::clean_name(s["name"].as_str()?);
                let address = with_port(s["address"].as_str().unwrap_or(&host));
                Some(ServerEntry { name, address, up: s["up"].as_bool().unwrap_or(false), flying: s["flying"].as_u64().unwrap_or(0) as u32, ping_ms: None })
            })
            .collect(),
    )
}

/// The best of two Hello / Welcome round trips (ms), None if it never answers.
fn ping(server: &str) -> Option<u32> {
    let addr = server.to_socket_addrs().ok()?.next()?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).ok()?;
    sock.connect(addr).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    let hello = Msg::Hello { name: String::new() }.encode();
    let mut buf = [0u8; 256];
    let mut best: Option<u32> = None;
    for _ in 0..2 {
        let t0 = Instant::now();
        if sock.send(&hello).is_err() {
            continue;
        }
        if let Ok(n) = sock.recv(&mut buf) {
            if matches!(Msg::decode(&buf[..n]), Some(Msg::Welcome { .. })) {
                let ms = t0.elapsed().as_millis() as u32;
                best = Some(best.map_or(ms, |b| b.min(ms)));
            }
        }
    }
    best
}

/// Which traffic model stands for each of our aircraft (airmodels::TYPES:
/// light single, business jet, regional, narrow-body, wide-body).
pub fn size_of(aircraft: u8) -> u8 {
    match aircraft {
        0 | 8 => 0,
        1 => 4,
        6 => 2,
        _ => 1,
    }
}

/// How far below the body frame's origin the wheels touch, for an aircraft id
/// (the traffic models stand on their wheels at their origin).
fn wheels_below(aircraft: u8) -> f64 {
    crate::aircraft::BUILTIN.iter().find(|s| s.id == aircraft as u32).map_or(1.2, |s| -s.gear.iter().map(|l| l.at.y - l.wheel_r).fold(0.0, f64::min))
}

/// The other players as traffic models with their lights (as live.rs draws
/// the real traffic). Those `full` are drawn with their own model already
/// (main.rs `draw_mp_planes`): only their lights' glow is added here.
pub fn instances(players: &[Remote], full: &[bool], cam: DVec3, out: &mut Vec<(u32, Inst)>, lights: &mut Vec<Light>) {
    for (i, p) in players.iter().enumerate() {
        let own_model = full.get(i).copied().unwrap_or(false);
        if (p.pos - cam).length() > 40_000.0 {
            continue;
        }
        let (x, y, z) = ((p.rot * DVec3::X).as_vec3(), (p.rot * DVec3::Y).as_vec3(), (p.rot * DVec3::Z).as_vec3());
        let base = p.pos + p.rot * DVec3::new(0.0, -wheels_below(p.aircraft), 0.0);
        let o = (base - cam).as_vec3();
        let inst = |tint: [f32; 3]| Inst { m: [[x.x, y.x, z.x, o.x], [x.y, y.y, z.y, o.y], [x.z, y.z, z.z, o.z]], tint: [tint[0], tint[1], tint[2], 0.0], ext: [0.0; 4] };
        let size = size_of(p.aircraft);
        let m = am::M_BODY + 3 * size as u32;
        // Each player gets a fin colour of their own.
        if !own_model {
            out.push((m, inst(crate::traffic::BODY[0])));
            out.push((m + 1, inst(crate::traffic::FIN[p.id as usize % crate::traffic::FIN.len()])));
            if p.gear > 0.5 {
                out.push((m + 2, inst([0.5; 3])));
            }
        }
        let ty = TYPES[size as usize];
        let at = |q: Vec3| -> [f32; 3] { (o + x * q.x + y * q.y - z * q.z).into() };
        let mut l = |q: Vec3, inten: f32, col: [u8; 3], fx: [u8; 4]| {
            if lights.len() < crate::render::MAX_DYN_LIGHTS {
                lights.push(Light { pos: at(q), inten, dir: [0.0; 3], spread: 0.0, col: [col[0], col[1], col[2], 0], fx });
            }
        };
        let steady = [0, 0, 255, 1];
        let ph = ((i * 53) % 255) as u8;
        if p.flags & proto::F_NAV != 0 {
            l(Vec3::new(-ty.tip.x, ty.tip.y, -ty.tip.z), 40.0, L_RED, steady);
            l(Vec3::new(ty.tip.x, ty.tip.y, -ty.tip.z), 40.0, L_GREEN, steady);
            l(Vec3::new(0.0, ty.tail.y * 0.35, -ty.tail.z), 25.0, L_WHITE, steady);
        }
        if p.flags & proto::F_BEACON != 0 {
            l(Vec3::new(0.0, ty.cy + ty.rf + 0.1, 0.0), 120.0, L_RED, [12, ph, 25, 1]);
        }
        if p.flags & proto::F_STROBE != 0 {
            for sx in [-1.0f32, 1.0] {
                l(Vec3::new(sx * ty.tip.x, ty.tip.y, -ty.tip.z), 1600.0, L_STROBE, [11, ph.wrapping_add(60), 7, 1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(t: f64, x: f64, seq: u32) -> (f64, PlayerState) {
        (t, PlayerState { seq, pos: [6_372_000.0, x, 0.0], rot: [0.0, 0.0, 0.0, 1.0], vel: [0.0, 50.0, 0.0], ..Default::default() })
    }

    #[test]
    fn players_are_smoothed_between_reports_and_carried_on_after() {
        let p = Peer { name: "a".into(), snaps: VecDeque::from([snap(1.0, 0.0, 1), snap(1.1, 5.0, 2)]) };
        assert!((at(1, &p, 1.05).unwrap().pos.y - 2.5).abs() < 1e-6, "halfway");
        assert!((at(1, &p, 1.3).unwrap().pos.y - 15.0).abs() < 1e-6, "flown on at 50 m/s");
        assert!((at(1, &p, 9.0).unwrap().pos.y - 55.0).abs() < 1e-6, "for a second at most");
    }

    #[test]
    fn random_names_are_valid_and_vary() {
        let names: std::collections::HashSet<String> = (0..50).map(|_| random_name()).collect();
        assert!(names.len() > 40, "{names:?}");
        for n in &names {
            assert!(n.len() <= proto::NAME_MAX && proto::clean_name(n) == *n && n.chars().all(|c| c.is_ascii_alphanumeric()), "{n}");
        }
    }

    #[test]
    fn the_server_list_comes_from_every_main_relay() {
        let want: Vec<String> = proto::MAIN_RELAYS.iter().map(|h| format!("https://{}/servers.json", h.split(':').next().unwrap())).collect();
        assert_eq!(DIRECTORY_URLS, want, "one list URL per main relay (mp_proto::MAIN_RELAYS)");
    }

    #[test]
    fn the_server_list_is_read() {
        let j = r#"{"servers":[{"name":"France · danux.be","address":null,"up":true,"flying":3},{"name":"US","address":"203.0.113.9:7480","up":false,"flying":0},{"name":"Asia","address":"asia.example.com","up":true,"flying":1}]}"#.as_bytes();
        let l = parse_servers(j, "https://fableflight.danux.be/servers.json").unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].name.as_str(), l[0].address.as_str(), l[0].flying), ("France · danux.be", "fableflight.danux.be:7480", 3));
        assert_eq!((l[1].address.as_str(), l[1].up), ("203.0.113.9:7480", false));
        assert_eq!(l[2].address, "asia.example.com:7480");
        assert!(parse_servers(b"nonsense", DIRECTORY_URLS[0]).is_none());
    }

    /// A minimal relay on `port` until `stop`: Welcome to a Hello, and each
    /// State answered with an empty Peers (it is up).
    fn tiny_relay(port: u16, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
        let sock = UdpSocket::bind(("127.0.0.1", port)).unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while !stop.load(Ordering::Relaxed) {
                let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
                match Msg::decode(&buf[..n]) {
                    Some(Msg::Hello { .. }) => {
                        let _ = sock.send_to(&Msg::Welcome { id: 1, cookie: 7 }.encode(), from);
                    }
                    Some(Msg::State { .. }) => {
                        let _ = sock.send_to(&Msg::Peers { peers: Vec::new() }.encode(), from);
                    }
                    _ => {}
                }
            }
        })
    }

    #[test]
    fn a_client_comes_back_after_its_relay_restarts() {
        let port = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let wait = |c: &Mp, want: Status, secs: u64| {
            let t0 = Instant::now();
            while c.status() != want && t0.elapsed() < Duration::from_secs(secs) {
                std::thread::sleep(Duration::from_millis(50));
            }
            c.status() == want
        };
        let stop = Arc::new(AtomicBool::new(false));
        let h = tiny_relay(port, stop.clone());
        let c = Mp::start(&format!("127.0.0.1:{port}"), "A");
        c.set_state(PlayerState { pos: [6_372_000.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], ..Default::default() });
        assert!(wait(&c, Status::Connected, 5), "connects");
        // The relay goes down: the client notices within LOST and starts over.
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        assert!(wait(&c, Status::Connecting, 8), "notices the relay is gone");
        // It comes back (a new run, new cookies): the client is in again by itself.
        let stop = Arc::new(AtomicBool::new(false));
        let _h = tiny_relay(port, stop.clone());
        assert!(wait(&c, Status::Connected, 6), "back without anyone doing anything");
        stop.store(true, Ordering::Relaxed);
    }

    #[test]
    fn without_a_list_server_the_last_list_stands_in() {
        // A relay that answers Hello, and a list that names it and one that is gone.
        let relay = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = relay.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            while let Ok((n, from)) = relay.recv_from(&mut buf) {
                if let Some(Msg::Hello { .. }) = Msg::decode(&buf[..n]) {
                    let _ = relay.send_to(&Msg::Welcome { id: 1, cookie: 2 }.encode(), from);
                }
            }
        });
        let dir = std::env::temp_dir().join(format!("ff-mp-cache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cache = dir.join("servers.json");
        let list = format!(r#"{{"servers":[{{"name":"EU","address":"127.0.0.1:{port}","up":true,"flying":4}},{{"name":"Gone","address":"127.0.0.1:9","up":true,"flying":0}}]}}"#);
        let urls = vec!["https://down.example/servers.json".to_string(), "https://up.example/servers.json".to_string()];
        // The first list server is down, the second answers: its list is used and kept.
        let (got, err) = read_list(&urls, |u| if u.contains("up.") { Ok(list.clone().into_bytes()) } else { Err("down".into()) }, Some(&cache));
        let (l, stale) = got.unwrap();
        assert!(!stale && err.is_none() && l.len() == 2 && l[0].ping_ms.is_some() && l[0].flying == 4);
        // Now none answers: the kept list stands in, its servers asked directly.
        let (got, err) = read_list(&urls, |_| Err("down".into()), Some(&cache));
        let (l, stale) = got.unwrap();
        assert!(stale && err.is_some());
        assert!(l[0].up && l[0].ping_ms.is_some(), "the live relay answers");
        assert!(!l[1].up && l[1].ping_ms.is_none(), "the gone one does not");
        // Nothing kept and nothing answering: no list.
        let _ = std::fs::remove_file(&cache);
        assert!(read_list(&urls, |_| Err("down".into()), Some(&cache)).0.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_names_list_matches_the_aircraft() {
        for s in crate::aircraft::BUILTIN.iter() {
            assert_eq!(proto::AIRCRAFT.get(s.id as usize).copied(), Some(s.name), "id {}", s.id);
        }
        assert_eq!(proto::PLANET_R, crate::geo::R);
    }

    /// A relay answering Hello; with `world`, a Watch gets two players on
    /// the world map back (else it is ignored, as by a relay from before it).
    /// It counts the States it gets.
    fn watch_relay(world: bool) -> (u16, Arc<std::sync::atomic::AtomicU32>) {
        let relay = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = relay.local_addr().unwrap().port();
        let states = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let n = states.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok((len, from)) = relay.recv_from(&mut buf) {
                match Msg::decode(&buf[..len]) {
                    Some(Msg::Hello { .. }) => {
                        let _ = relay.send_to(&Msg::Welcome { id: 1, cookie: 9 }.encode(), from);
                    }
                    Some(Msg::State { .. }) => {
                        n.fetch_add(1, Ordering::Relaxed);
                    }
                    Some(Msg::Watch { cookie: 9 }) if world => {
                        let s = PlayerState { pos: [0.0, 0.0, 6_372_000.0], rot: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
                        let spots = vec![proto::Spot::of(4, "Dany", &s), proto::Spot { id: 5, name: "Roland".into(), lat: 16000, ..Default::default() }];
                        let _ = relay.send_to(&Msg::World { total: 7, spots }.encode(), from);
                    }
                    _ => {}
                }
            }
        });
        (port, states)
    }

    #[test]
    fn on_the_start_screen_the_world_comes_in_and_nothing_goes_out() {
        let (port, states) = watch_relay(true);
        let c = Mp::start(&format!("127.0.0.1:{port}"), "A");
        c.set_state(PlayerState { pos: [6_372_000.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], ..Default::default() });
        c.set_watching(true);
        let t0 = Instant::now();
        while c.world().0.len() < 2 && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(30));
        }
        let (spots, total, known) = c.world();
        assert_eq!((spots.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), total, known), (vec!["Dany", "Roland"], 7, true));
        assert!((spots[0].lat_deg() - 90.0).abs() < 0.01);
        assert_eq!(c.status(), Status::Connected);
        assert_eq!(states.load(Ordering::Relaxed), 0, "the aircraft is not sent from the menu");
        // Off to fly: the aircraft goes out again, the world map is let go.
        c.set_watching(false);
        assert!(c.world().0.is_empty());
        c.set_state(PlayerState { pos: [6_372_000.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], ..Default::default() });
        let t0 = Instant::now();
        while states.load(Ordering::Relaxed) == 0 && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(states.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn an_older_relay_is_connected_without_a_world_map() {
        let (port, _) = watch_relay(false);
        let c = Mp::start(&format!("127.0.0.1:{port}"), "A");
        c.set_watching(true);
        let t0 = Instant::now();
        while c.status() != Status::Connected && t0.elapsed() < Duration::from_secs(4) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(c.status(), Status::Connected);
        // (past LOST it says Hello again, without looking lost)
        std::thread::sleep(LOST + Duration::from_millis(1500));
        assert_eq!(c.status(), Status::Connected);
        assert_eq!(c.world(), (Vec::new(), 0, false));
    }

    #[test]
    fn a_spot_agrees_with_the_games_frame() {
        let (lat, lon) = (-33.9f64, 151.2f64);
        let pos = crate::geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 2500.0);
        let (e, n, u) = crate::geo::enu(pos);
        // heading 300°: forward in the world, and the body's right, up and back
        let h = 300f64.to_radians();
        let fwd = n * h.cos() + e * h.sin();
        let right = fwd.cross(u);
        let q = DQuat::from_mat3(&glam::DMat3::from_cols(right, u, -fwd));
        let s = PlayerState { pos: pos.to_array(), rot: [q.x as f32, q.y as f32, q.z as f32, q.w as f32], vel: (fwd * 60.0).as_vec3().to_array(), ..Default::default() };
        let p = proto::Spot::of(1, "x", &s);
        assert!((p.lat_deg() - lat).abs() < 0.01 && (p.lon_deg() - lon).abs() < 0.01);
        assert!((p.heading_deg() - 300.0).abs() < 1.5 && p.alt_m() == 2500.0 && p.speed == 117, "{p:?}");
    }

    #[test]
    fn two_players_meet_through_a_relay() {
        // A minimal relay in a thread: it answers Hello and passes each
        // State on to the other address as Peers (the real one is relay/).
        let relay = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = relay.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let mut who: Vec<std::net::SocketAddr> = Vec::new();
            loop {
                let Ok((n, from)) = relay.recv_from(&mut buf) else { break };
                match Msg::decode(&buf[..n]) {
                    Some(Msg::Hello { .. }) => {
                        if !who.contains(&from) {
                            who.push(from);
                        }
                        let _ = relay.send_to(&Msg::Welcome { id: who.len() as u32, cookie: 7 }.encode(), from);
                    }
                    Some(Msg::State { state, .. }) => {
                        let me = who.iter().position(|a| *a == from).unwrap_or(0) as u32 + 1;
                        for a in who.iter().filter(|a| **a != from) {
                            let _ = relay.send_to(&Msg::Peers { peers: vec![(me, format!("p{me}"), state)] }.encode(), a);
                        }
                    }
                    _ => {}
                }
            }
        });
        let server = format!("127.0.0.1:{port}");
        let (a, b) = (Mp::start(&server, "A"), Mp::start(&server, "B"));
        let s = PlayerState { pos: [6_372_000.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0, 1.0], aircraft: 8, ..Default::default() };
        a.set_state(s);
        b.set_state(s);
        let t0 = Instant::now();
        while (a.players().is_empty() || b.players().is_empty()) && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(a.status(), Status::Connected);
        assert_eq!(b.players().len(), 1);
        assert_eq!(a.players()[0].aircraft, 8);
    }
}
