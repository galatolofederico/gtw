//! The multiplayer protocol, shared word for word by the game (`mp.rs`) and
//! the relay (`relay/`, which includes this file): small binary UDP
//! datagrams, little-endian, each starting with `FFMP` and the version.
//!
//! - `Hello` (client → relay): "I want in, call me this". Padded to at least
//!   `HELLO_MIN` bytes, so the answer is never bigger than the question (a
//!   forged sender gets nothing amplified).
//! - `Welcome` (relay → client): your id and a cookie. Nothing else is ever
//!   sent to an address until it has sent that cookie back.
//! - `State` (client → relay, ~15 a second): where the aircraft is and how
//!   it is set up, with the cookie.
//! - `Peers` (relay → client, ~10 a second): the other players nearby.
//! - `Bye` (client → relay): leaving.
//! - `Watch` (client → relay, about once a second, with the cookie): "I am
//!   not flying, show me the world" (the start screen's globe). It also
//!   takes the sender off the players flying, if it was one.
//! - `World` (relay → client, once a second to whoever sent a Watch in the
//!   last few seconds): every player of the network, coarsely (`Spot`).
//!   Relays from before it ignore a Watch (unknown kinds are dropped).
//!
//! Only std, no allocation beyond the Vec of a packet: the relay builds it
//! with nothing else.

pub const MAGIC: [u8; 4] = *b"FFMP";
pub const VERSION: u8 = 1;
pub const DEFAULT_PORT: u16 = 7480;
/// The network's main relays (`host[:port]`), for redundancy: every relay
/// with the network's secret makes itself known to all of them (skipping
/// itself), so that it stays in the network while any one is up. The game
/// reads its server list from the same hosts (`mp::DIRECTORY_URLS`).
#[allow(dead_code)] // (the relay's; the game checks its list URLs against it)
pub const MAIN_RELAYS: &[&str] = &["fableflight.danux.be", "fableflight.rolandiscool.com"];
/// the longest name, in bytes (UTF-8, printable)
pub const NAME_MAX: usize = 24;
/// a Hello is at least this long (see the module notes)
pub const HELLO_MIN: usize = 48;
/// at most this many players in one Peers packet (it stays under ~1100 bytes)
pub const PEERS_PER_PACKET: usize = 10;
/// at most this many relays in one Directory packet (likewise)
pub const DIRECTORY_PER_PACKET: usize = 20;
/// at most this many players in one World packet (likewise)
pub const WORLD_PER_PACKET: usize = 24;

/// The aircraft by `PlayerState::aircraft` (`aircraft::Spec::id`), for the
/// relay's status page; ids past the end are shown as unknown. (The game's
/// tests check this list against its aircraft.)
pub const AIRCRAFT: &[&str] = &["Cessna 172RG", "Boeing 747-400", "F/A-18C Hornet", "F-4E Phantom II", "F-22A Raptor", "SpaceX Starship", "Concorde", "SR-71A Blackbird", "Robin DR 400/180", "Gulfstream G650", "Beechcraft A36 Bonanza"];
/// the planet's radius (m), as the game uses it (geo.rs)
#[allow(dead_code)] // (the relay's status page uses it)
pub const PLANET_R: f64 = 6_371_008.8;

const K_HELLO: u8 = 1;
const K_WELCOME: u8 = 2;
const K_STATE: u8 = 3;
const K_PEERS: u8 = 4;
const K_BYE: u8 = 5;
const K_FED: u8 = 6;
const K_DIRECTORY: u8 = 7;
const K_WATCH: u8 = 8;
const K_WORLD: u8 = 9;

/// One player's aircraft at one instant. Positions are in the game's world
/// frame (metres from the planet's centre), the attitude a unit quaternion
/// (x, y, z, w) from the body frame to the world frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    /// counts up with each State the client sends
    pub seq: u32,
    /// which aircraft (`aircraft::Spec::id`)
    pub aircraft: u8,
    pub pos: [f64; 3],
    pub rot: [f32; 4],
    /// velocity (m/s, world frame)
    pub vel: [f32; 3],
    /// 0..255: flaps, gear, throttle, canopy
    pub flaps: u8,
    pub gear: u8,
    pub throttle: u8,
    pub canopy: u8,
    /// -127..127: elevator, aileron, rudder
    pub pitch: i8,
    pub roll: i8,
    pub yaw: i8,
    /// bits: 1 navigation lights, 2 strobes, 4 beacon, 8 landing lights,
    /// 16 on the ground, 32 crashed
    pub flags: u8,
}
#[allow(dead_code)] // (a State's size on the wire; the tests check it)
pub const STATE_LEN: usize = 4 + 1 + 24 + 16 + 12 + 4 + 3 + 1;

pub const F_NAV: u8 = 1;
pub const F_STROBE: u8 = 2;
pub const F_BEACON: u8 = 4;
pub const F_LANDING: u8 = 8;
pub const F_GROUND: u8 = 16;
pub const F_CRASHED: u8 = 32;

/// A player as the world map shows them (`World`): where, coarsely (to
/// about 600 m), in what, how high, which way and how fast. Coarse on
/// purpose: the exact state only goes to the players near them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spot {
    pub id: u32,
    pub name: String,
    pub aircraft: u8,
    /// latitude and longitude, in 32767ths of 90° and of 180°
    pub lat: i16,
    pub lon: i16,
    /// height above the sphere, in tens of metres (up to 655 km)
    pub alt: u16,
    /// heading, in 256ths of a turn
    pub heading: u8,
    /// speed (kt)
    pub speed: u16,
    /// `F_GROUND` and `F_CRASHED` from the state
    pub flags: u8,
}

impl Spot {
    /// The spot of a player whose state is `s`.
    #[allow(dead_code)] // (the relay's; the game's tests check it against geo.rs)
    pub fn of(id: u32, name: &str, s: &PlayerState) -> Spot {
        let [x, y, z] = s.pos;
        let r = (x * x + y * y + z * z).sqrt().max(1.0);
        let up = [x / r, y / r, z / r];
        let (lat, lon) = (up[2].clamp(-1.0, 1.0).asin().to_degrees(), y.atan2(x).to_degrees());
        // Forward (body -Z) in the world, on the local east and north (as geo::enu).
        let [qx, qy, qz, qw] = s.rot.map(|v| v as f64);
        let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let v = [0.0, 0.0, -1.0];
        let t = cross([qx, qy, qz], v).map(|c| 2.0 * c);
        let u = cross([qx, qy, qz], t);
        let fwd = [v[0] + qw * t[0] + u[0], v[1] + qw * t[1] + u[1], v[2] + qw * t[2] + u[2]];
        let e = [-up[1], up[0], 0.0];
        let el = (e[0] * e[0] + e[1] * e[1]).sqrt();
        let east = if el < 1e-9 { [0.0, 1.0, 0.0] } else { [e[0] / el, e[1] / el, 0.0] };
        let north = cross(up, east);
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let hdg = dot(fwd, east).atan2(dot(fwd, north)).to_degrees().rem_euclid(360.0);
        let v = s.vel.iter().map(|c| (*c as f64).powi(2)).sum::<f64>().sqrt();
        Spot {
            id,
            name: clean_name(name),
            aircraft: s.aircraft,
            lat: (lat / 90.0 * 32767.0).round().clamp(-32767.0, 32767.0) as i16,
            lon: (lon / 180.0 * 32767.0).round().clamp(-32767.0, 32767.0) as i16,
            alt: ((r - PLANET_R) / 10.0).round().clamp(0.0, u16::MAX as f64) as u16,
            heading: (hdg / 360.0 * 256.0).round() as u32 as u8,
            speed: (v * 1.94384).round().clamp(0.0, u16::MAX as f64) as u16,
            flags: s.flags & (F_GROUND | F_CRASHED),
        }
    }
    pub fn lat_deg(&self) -> f64 {
        self.lat as f64 * 90.0 / 32767.0
    }
    pub fn lon_deg(&self) -> f64 {
        self.lon as f64 * 180.0 / 32767.0
    }
    pub fn alt_m(&self) -> f64 {
        self.alt as f64 * 10.0
    }
    pub fn heading_deg(&self) -> f64 {
        self.heading as f64 * 360.0 / 256.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Hello { name: String },
    Welcome { id: u32, cookie: u64 },
    State { cookie: u64, state: PlayerState },
    Peers { peers: Vec<(u32, String, PlayerState)> },
    Bye { cookie: u64 },
    /// Relay → relay (federation, the game never sees one): the sending
    /// relay's own players. `relay` is that relay's id for this run, `ts`
    /// its clock (unix seconds), `seq` counts up; `tag` signs everything
    /// before it with the network's shared secret (see `fed_tag`).
    Fed { relay: u32, relay_name: String, ts: u64, seq: u32, players: Vec<(u32, String, PlayerState)>, tag: u64 },
    /// Relay → relay: the other relays the sender knows (their id, address
    /// as it sees it, name), so that every relay of the network finds every
    /// other. Signed and numbered as a Fed.
    Directory { relay: u32, ts: u64, seq: u32, members: Vec<(u32, std::net::SocketAddr, String)>, tag: u64 },
    /// Client → relay: show me the world (see the module notes).
    Watch { cookie: u64 },
    /// Relay → client: some of the network's players (`total` in all; a
    /// round of several packets lists them all, up to the relay's limit).
    World { total: u32, spots: Vec<Spot> },
}

/// A name as it may be shown: printable characters only, trimmed, at most
/// `NAME_MAX` bytes (cut on a character boundary).
pub fn clean_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > NAME_MAX {
            break;
        }
        out.push(c);
    }
    out
}

struct W(Vec<u8>);
impl W {
    fn new(kind: u8) -> Self {
        let mut v = Vec::with_capacity(128);
        v.extend_from_slice(&MAGIC);
        v.push(VERSION);
        v.push(kind);
        W(v)
    }
    fn u8(&mut self, x: u8) {
        self.0.push(x);
    }
    fn u32(&mut self, x: u32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn u64(&mut self, x: u64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn f32(&mut self, x: f32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn f64(&mut self, x: f64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn name(&mut self, s: &str) {
        let s = clean_name(s);
        self.u8(s.len() as u8);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn state(&mut self, s: &PlayerState) {
        self.u32(s.seq);
        self.u8(s.aircraft);
        s.pos.iter().for_each(|&x| self.f64(x));
        s.rot.iter().for_each(|&x| self.f32(x));
        s.vel.iter().for_each(|&x| self.f32(x));
        for x in [s.flaps, s.gear, s.throttle, s.canopy] {
            self.u8(x);
        }
        for x in [s.pitch, s.roll, s.yaw] {
            self.u8(x as u8);
        }
        self.u8(s.flags);
    }
}

struct R<'a>(&'a [u8]);
impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|x| x.is_finite())
    }
    fn f64(&mut self) -> Option<f64> {
        self.take(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).filter(|x| x.is_finite())
    }
    fn name(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        if n > NAME_MAX {
            return None;
        }
        let s = std::str::from_utf8(self.take(n)?).ok()?;
        Some(clean_name(s))
    }
    fn state(&mut self) -> Option<PlayerState> {
        let mut s = PlayerState { seq: self.u32()?, aircraft: self.u8()?, ..Default::default() };
        for x in s.pos.iter_mut() {
            *x = self.f64()?;
        }
        for x in s.rot.iter_mut() {
            *x = self.f32()?;
        }
        for x in s.vel.iter_mut() {
            *x = self.f32()?;
        }
        (s.flaps, s.gear, s.throttle, s.canopy) = (self.u8()?, self.u8()?, self.u8()?, self.u8()?);
        (s.pitch, s.roll, s.yaw) = (self.u8()? as i8, self.u8()? as i8, self.u8()? as i8);
        s.flags = self.u8()?;
        // Somewhere near the planet (within 10 000 km of its centre, past its
        // core), moving slower than 10 km/s, with a usable attitude.
        let r = (s.pos[0] * s.pos[0] + s.pos[1] * s.pos[1] + s.pos[2] * s.pos[2]).sqrt();
        let q = s.rot.iter().map(|x| x * x).sum::<f32>().sqrt();
        let v = s.vel.iter().map(|x| x * x).sum::<f32>().sqrt();
        (r > 6.0e6 && r < 1.0e7 && (q - 1.0).abs() < 0.05 && v < 10_000.0).then_some(s)
    }
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Msg::Hello { name } => {
                let mut w = W::new(K_HELLO);
                w.name(name);
                w.0.resize(w.0.len().max(HELLO_MIN), 0);
                w.0
            }
            Msg::Welcome { id, cookie } => {
                let mut w = W::new(K_WELCOME);
                w.u32(*id);
                w.u64(*cookie);
                w.0
            }
            Msg::State { cookie, state } => {
                let mut w = W::new(K_STATE);
                w.u64(*cookie);
                w.state(state);
                w.0
            }
            Msg::Peers { peers } => {
                let mut w = W::new(K_PEERS);
                let n = peers.len().min(PEERS_PER_PACKET);
                w.u8(n as u8);
                for (id, name, s) in &peers[..n] {
                    w.u32(*id);
                    w.name(name);
                    w.state(s);
                }
                w.0
            }
            Msg::Bye { cookie } => {
                let mut w = W::new(K_BYE);
                w.u64(*cookie);
                w.0
            }
            Msg::Fed { relay, relay_name, ts, seq, players, tag } => {
                let mut w = W::new(K_FED);
                w.u32(*relay);
                w.name(relay_name);
                w.u64(*ts);
                w.u32(*seq);
                let n = players.len().min(PEERS_PER_PACKET);
                w.u8(n as u8);
                for (id, name, s) in &players[..n] {
                    w.u32(*id);
                    w.name(name);
                    w.state(s);
                }
                w.u64(*tag);
                w.0
            }
            Msg::Directory { relay, ts, seq, members, tag } => {
                let mut w = W::new(K_DIRECTORY);
                w.u32(*relay);
                w.u64(*ts);
                w.u32(*seq);
                let n = members.len().min(DIRECTORY_PER_PACKET);
                w.u8(n as u8);
                for (id, addr, name) in &members[..n] {
                    w.u32(*id);
                    match addr.ip() {
                        std::net::IpAddr::V4(ip) => {
                            w.u8(4);
                            w.0.extend_from_slice(&ip.octets());
                        }
                        std::net::IpAddr::V6(ip) => {
                            w.u8(6);
                            w.0.extend_from_slice(&ip.octets());
                        }
                    }
                    w.0.extend_from_slice(&addr.port().to_le_bytes());
                    w.name(name);
                }
                w.u64(*tag);
                w.0
            }
            Msg::Watch { cookie } => {
                let mut w = W::new(K_WATCH);
                w.u64(*cookie);
                w.0
            }
            Msg::World { total, spots } => {
                let mut w = W::new(K_WORLD);
                w.u32(*total);
                let n = spots.len().min(WORLD_PER_PACKET);
                w.u8(n as u8);
                for s in &spots[..n] {
                    w.u32(s.id);
                    w.name(&s.name);
                    w.u8(s.aircraft);
                    w.0.extend_from_slice(&s.lat.to_le_bytes());
                    w.0.extend_from_slice(&s.lon.to_le_bytes());
                    w.0.extend_from_slice(&s.alt.to_le_bytes());
                    w.u8(s.heading);
                    w.0.extend_from_slice(&s.speed.to_le_bytes());
                    w.u8(s.flags);
                }
                w.0
            }
        }
    }

    /// The message in a datagram; None for anything that is not a whole,
    /// sane message of this version.
    pub fn decode(b: &[u8]) -> Option<Msg> {
        let mut r = R(b);
        if r.take(4)? != MAGIC || r.u8()? != VERSION {
            return None;
        }
        let m = match r.u8()? {
            K_HELLO => {
                if b.len() < HELLO_MIN {
                    return None;
                }
                Msg::Hello { name: r.name()? }
            }
            K_WELCOME => Msg::Welcome { id: r.u32()?, cookie: r.u64()? },
            K_STATE => Msg::State { cookie: r.u64()?, state: r.state()? },
            K_PEERS => {
                let n = r.u8()? as usize;
                if n > PEERS_PER_PACKET {
                    return None;
                }
                let mut peers = Vec::with_capacity(n);
                for _ in 0..n {
                    peers.push((r.u32()?, r.name()?, r.state()?));
                }
                Msg::Peers { peers }
            }
            K_BYE => Msg::Bye { cookie: r.u64()? },
            K_FED => {
                let (relay, relay_name, ts, seq) = (r.u32()?, r.name()?, r.u64()?, r.u32()?);
                let n = r.u8()? as usize;
                if n > PEERS_PER_PACKET {
                    return None;
                }
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push((r.u32()?, r.name()?, r.state()?));
                }
                Msg::Fed { relay, relay_name, ts, seq, players, tag: r.u64()? }
            }
            K_DIRECTORY => {
                let (relay, ts, seq) = (r.u32()?, r.u64()?, r.u32()?);
                let n = r.u8()? as usize;
                if n > DIRECTORY_PER_PACKET {
                    return None;
                }
                let mut members = Vec::with_capacity(n);
                for _ in 0..n {
                    let id = r.u32()?;
                    let ip: std::net::IpAddr = match r.u8()? {
                        4 => <[u8; 4]>::try_from(r.take(4)?).ok()?.into(),
                        6 => <[u8; 16]>::try_from(r.take(16)?).ok()?.into(),
                        _ => return None,
                    };
                    let port = u16::from_le_bytes(r.take(2)?.try_into().ok()?);
                    members.push((id, std::net::SocketAddr::new(ip, port), r.name()?));
                }
                Msg::Directory { relay, ts, seq, members, tag: r.u64()? }
            }
            K_WATCH => Msg::Watch { cookie: r.u64()? },
            K_WORLD => {
                let total = r.u32()?;
                let n = r.u8()? as usize;
                if n > WORLD_PER_PACKET {
                    return None;
                }
                let mut spots = Vec::with_capacity(n);
                let i16 = |r: &mut R| r.take(2).map(|b| i16::from_le_bytes([b[0], b[1]]));
                let u16 = |r: &mut R| r.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]));
                for _ in 0..n {
                    let (id, name, aircraft) = (r.u32()?, r.name()?, r.u8()?);
                    let (lat, lon, alt, heading, speed, flags) = (i16(&mut r)?, i16(&mut r)?, u16(&mut r)?, r.u8()?, u16(&mut r)?, r.u8()?);
                    if lat == i16::MIN || lon == i16::MIN {
                        return None;
                    }
                    spots.push(Spot { id, name, aircraft, lat, lon, alt, heading, speed, flags });
                }
                Msg::World { total, spots }
            }
            _ => return None,
        };
        Some(m)
    }
}

/// The signature of a federation datagram: SipHash-2-4 (a keyed hash made
/// for this) of everything before the tag, keyed by the shared secret.
#[allow(dead_code, deprecated)] // (only the relay signs; SipHasher is std's own)
pub fn fed_tag(secret: &str, signed: &[u8]) -> u64 {
    use std::hash::{Hasher, SipHasher};
    let key = |salt: u64| {
        let mut h = SipHasher::new_with_keys(salt, !salt);
        h.write(secret.as_bytes());
        h.finish()
    };
    let mut h = SipHasher::new_with_keys(key(0x6666_6d70_6665_6431), key(0x6666_6d70_6665_6432));
    h.write(signed);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PlayerState {
        PlayerState { seq: 7, aircraft: 8, pos: [4_000_000.0, 300_000.0, 4_950_000.0], rot: [0.0, 0.0, 0.0, 1.0], vel: [10.0, -2.0, 55.0], flaps: 128, gear: 255, throttle: 200, canopy: 0, pitch: -40, roll: 12, yaw: 0, flags: F_NAV | F_BEACON }
    }

    #[test]
    fn every_message_comes_back_the_same() {
        for m in [
            Msg::Hello { name: "F-GXYZ".into() },
            Msg::Welcome { id: 3, cookie: 0xdead_beef_1234 },
            Msg::State { cookie: 99, state: state() },
            Msg::Peers { peers: vec![(1, "Dany".into(), state()), (2, "Roland".into(), state())] },
            Msg::Bye { cookie: 5 },
            Msg::Fed { relay: 77, relay_name: "danux.be".into(), ts: 1_800_000_000, seq: 4, players: vec![(1, "Dany".into(), state())], tag: 9 },
            Msg::Directory { relay: 77, ts: 1_800_000_000, seq: 5, members: vec![(3, "203.0.113.7:7480".parse().unwrap(), "Asia".into()), (4, "[2001:db8::1]:7480".parse().unwrap(), "US".into())], tag: 11 },
            Msg::Watch { cookie: 0x1234_5678_9abc },
            Msg::World { total: 40, spots: vec![Spot::of(1, "Dany", &state()), Spot { id: 2 << 24 | 5, name: "Roland".into(), aircraft: 1, lat: -32767, lon: 32767, alt: 1100, heading: 255, speed: 480, flags: F_GROUND }] },
            Msg::World { total: 0, spots: Vec::new() },
        ] {
            let b = m.encode();
            assert_eq!(Msg::decode(&b), Some(m.clone()), "{m:?}");
        }
        assert_eq!(Msg::State { cookie: 1, state: state() }.encode().len(), 6 + 8 + STATE_LEN);
    }

    #[test]
    fn the_answer_to_a_hello_is_never_bigger() {
        let hello = Msg::Hello { name: String::new() }.encode();
        let welcome = Msg::Welcome { id: u32::MAX, cookie: u64::MAX }.encode();
        assert!(hello.len() >= HELLO_MIN && welcome.len() <= hello.len());
        // A short Hello is not one.
        assert_eq!(Msg::decode(&hello[..HELLO_MIN - 1]), None);
    }

    #[test]
    fn junk_is_turned_away() {
        let good = Msg::State { cookie: 1, state: state() }.encode();
        for n in 0..good.len() {
            assert_eq!(Msg::decode(&good[..n]), None, "cut at {n}");
        }
        let mut v = good.clone();
        v[4] = VERSION + 1;
        assert_eq!(Msg::decode(&v), None);
        let mut s = state();
        s.pos[0] = f64::NAN;
        assert_eq!(Msg::decode(&Msg::State { cookie: 1, state: s }.encode()), None);
        s = state();
        s.pos = [1.0, 2.0, 3.0];
        assert_eq!(Msg::decode(&Msg::State { cookie: 1, state: s }.encode()), None);
        s = state();
        s.rot = [0.0, 0.0, 0.0, 3.0];
        assert_eq!(Msg::decode(&Msg::State { cookie: 1, state: s }.encode()), None);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Dany\n\u{7}  "), "Dany");
        assert_eq!(clean_name(&"é".repeat(40)).len(), NAME_MAX);
        let peers = (0..20).map(|i| (i, "x".repeat(NAME_MAX), state())).collect();
        let b = Msg::Peers { peers }.encode();
        assert!(b.len() < 1200, "{}", b.len());
        let spots = (0..40).map(|i| Spot { id: i, name: "x".repeat(NAME_MAX), ..Default::default() }).collect();
        let b = Msg::World { total: 40, spots }.encode();
        assert!(b.len() < 1200, "{}", b.len());
        assert_eq!(Msg::decode(&b), Some(Msg::World { total: 40, spots: (0..WORLD_PER_PACKET as u32).map(|i| Spot { id: i, name: "x".repeat(NAME_MAX), ..Default::default() }).collect() }));
    }

    #[test]
    fn a_spot_is_where_the_player_is() {
        // Over 45°N 10°E, 1000 m up, flying north-east at 100 m/s.
        let (la, lo) = (45f64.to_radians(), 10f64.to_radians());
        let r = PLANET_R + 1000.0;
        let up = [la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()];
        let east = [-lo.sin(), lo.cos(), 0.0];
        let north = [-la.sin() * lo.cos(), -la.sin() * lo.sin(), la.cos()];
        let h = 45f64.to_radians();
        let fwd: Vec<f64> = (0..3).map(|i| north[i] * h.cos() + east[i] * h.sin()).collect();
        // body axes in the world: +X right, +Y up, -Z forward
        let right: Vec<f64> = (0..3).map(|i| east[i] * h.cos() - north[i] * h.sin()).collect();
        let back: Vec<f64> = fwd.iter().map(|x| -x).collect();
        // (rotation matrix columns right, up, back to a quaternion)
        let m = [[right[0], up[0], back[0]], [right[1], up[1], back[1]], [right[2], up[2], back[2]]];
        let w = (1.0 + m[0][0] + m[1][1] + m[2][2]).max(0.0).sqrt() / 2.0;
        let q = if w > 0.1 {
            [(m[2][1] - m[1][2]) / (4.0 * w), (m[0][2] - m[2][0]) / (4.0 * w), (m[1][0] - m[0][1]) / (4.0 * w), w]
        } else {
            let x = (1.0 + m[0][0] - m[1][1] - m[2][2]).max(0.0).sqrt() / 2.0;
            [x, (m[0][1] + m[1][0]) / (4.0 * x), (m[0][2] + m[2][0]) / (4.0 * x), (m[2][1] - m[1][2]) / (4.0 * x)]
        };
        let s = PlayerState { pos: up.map(|c| c * r), rot: q.map(|c| c as f32), vel: [fwd[0] as f32 * 100.0, fwd[1] as f32 * 100.0, fwd[2] as f32 * 100.0], flags: F_NAV | F_GROUND, ..state() };
        let p = Spot::of(3, "  Dany ", &s);
        assert!((p.lat_deg() - 45.0).abs() < 0.01 && (p.lon_deg() - 10.0).abs() < 0.01, "{} {}", p.lat_deg(), p.lon_deg());
        assert_eq!((p.alt_m(), p.speed, p.flags, p.name.as_str()), (1000.0, 194, F_GROUND, "Dany"));
        assert!((p.heading_deg() - 45.0).abs() < 1.5, "{}", p.heading_deg());
    }
}
