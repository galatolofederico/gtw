//! Thunderstorms up close: what the storm cells (weather.rs `storm_cells`) are
//! doing where the aircraft is (rain, cloud, up- and downdrafts), their
//! lightning, and the rain falling around the camera.
//!
//! Lightning: each mature cell flashes now and then, more often the bigger
//! and older it is. Most flashes stay in the cloud: a few pulses over a
//! fraction of a second, each lighting a different stretch of it from inside.
//! The rest come down to the ground: a stepped leader feels its way down
//! (faint), the return stroke blazes up the channel and its branches, and one
//! to four more strokes follow down the main channel tens of milliseconds
//! apart, each leaving a short afterglow. The channels are drawn by
//! precip.wgsl (`vs_bolt`); the light they throw on the clouds and the ground
//! goes to the shaders as up to `MAX_FLASHES` lit spans (common.wgsl
//! `flash_light`). The thunder follows at the speed of sound (audio.rs).
//!
//! The geometry of a cell here must match the shaders': the tower (common.wgsl
//! `storm_sd`), leaning downwind, and the rain shaft under its downwind half
//! (clouds.wgsl `storm_rain`).
use crate::geo;
use crate::weather::Weather;
use glam::DVec3;

/// Lit spans handed to the shaders at once (two vec4s each in Globals.flash).
pub const MAX_FLASHES: usize = 4;
/// Most lightning channel segments drawn in a frame.
pub const MAX_SEGS: usize = 4096;
/// Speed of sound, m/s.
const SOUND: f64 = 343.0;

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What a storm cell's air is doing at one point.
#[derive(Clone, Copy, Debug, Default)]
pub struct Local {
    /// rain falling here, 0..1 (heavy under the core, light at the shaft's edge)
    pub rain: f64,
    /// inside the storm tower's cloud, 0..1
    pub cloud: f64,
    /// RMS turbulence the storm adds, m/s
    pub sigma: f64,
    /// vertical air motion, m/s (+ up): the updraft feeding the tower, the downdraft in the rain
    pub draft: f64,
    /// outflow along the ground from the downdraft (the gust front), planet frame m/s
    pub gust: DVec3,
    /// how much of the rain here is hail, 0..1 (a supercell's core)
    pub hail: f64,
}

/// A cell as seen from a point: (metres downwind of its centre, metres across
/// the wind, metres above the cloud base, tower radius m, maturity 0..1,
/// height of its tower above the base m), and the downwind and across axes.
/// A squall line (common.wgsl `storm_sd`) lies across the wind, `len2` metres
/// either side of its centre: `cxl` is along it, `cx` how far past its nearer
/// end (for a lone cell the two are the same).
struct At {
    dw0: f64,
    cx: f64,
    cxl: f64,
    len2: f64,
    seed: f64,
    hb: f64,
    r: f64,
    age: f64,
    htop: f64,
    /// how much of a supercell it is, 0..1 (weather.rs `supercell`)
    sup: f64,
    w: DVec3,
    x: DVec3,
}

/// The direction the wind aloft blows towards at `pos` (Globals.storm.xyz).
pub fn downwind(wx: &Weather, pos: DVec3) -> DVec3 {
    let w = wx.steady(pos, 2000.0);
    if w.length() > 0.1 { w.normalize() } else { geo::enu(pos).0 }
}

use crate::weather::storm_top_share as storm_top;

/// Lightning needs a tower reaching well above the freezing level, where ice
/// and graupel collide: cells topping out below about this (m above sea
/// level) are showers and never flash.
pub const LIGHTNING_TOP: f64 = 7500.0;

fn cell_at(wx: &Weather, c: &[[f32; 4]; 2], pos: DVec3, w: DVec3) -> At {
    let un = pos.normalize();
    let x = un.cross(w).normalize_or_zero();
    let cen = DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64);
    let dv = (un * 6371.0 + wx.cloud_off / 1000.0 - cen) * 1000.0;
    let alt = pos.length() - geo::R;
    let age = c[1][0] as f64;
    let at = c[1][1] as f64 - wx.base() as f64;
    let (len2, seed) = line_of(c);
    let cxl = dv.dot(x);
    let cx = cxl.signum() * (cxl.abs() - len2).max(0.0);
    At { dw0: dv.dot(w), cx, cxl, len2, seed, hb: alt - wx.base() as f64, r: c[0][3] as f64 * 1000.0, age, htop: at * storm_top(age), sup: crate::weather::supercell(c), w, x }
}

/// A supercell's rain and hail core: its centre downwind and to the left of
/// the cell's centre, and its radius, x the tower radius (clouds.wgsl
/// storm_rain; for sup = 0 an ordinary cell's shaft).
fn shaft_at(sup: f64) -> (f64, f64, f64) {
    (0.45 - 0.13 * sup, 0.28 * sup, 0.8 - 0.38 * sup)
}

/// A cell's squall-line half-length (m, 0 for a lone cell) and its seed: the
/// whole and fractional parts of its 8th value (weather.rs).
pub fn line_of(c: &[[f32; 4]; 2]) -> (f64, f64) {
    let v = c[1][3] as f64;
    (v.floor() * 500.0, v - v.floor())
}

/// A squall line's shelf cloud: its leading edge (the gust front) this far
/// ahead of the line's axis, and its width, x the line's radius (common.wgsl
/// SHELF_F, SHELF_W).
pub const SHELF_F: f64 = 1.42;
const SHELF_W: f64 = 0.50;

/// How far back from a squall line's gust front a point is, m (negative
/// ahead of it); the front bows in and out along the line (common.wgsl storm_sd).
fn shelf_u(a: &At) -> f64 {
    a.r * (SHELF_F + 0.12 * (a.cxl / 9000.0 + a.seed * 40.0).sin() + 0.07 * (a.cxl / 3700.0 + a.seed * 17.0).sin()) - a.dw0
}

/// A squall line's rain `u` m back from its gust front (radius `r`): how
/// much, and how much of that is the heavy curtain (clouds.wgsl line_rain).
fn line_rain(u: f64, r: f64) -> (f64, f64) {
    let ws = SHELF_W * r;
    let heavy = 1.0 - smoothstep(ws + 0.45 * r, ws + 0.95 * r, u);
    let tail = 0.35 * (1.0 - smoothstep(ws + 0.95 * r, ws + 3.2 * r, u));
    let on = smoothstep(0.7 * ws, 1.1 * ws, u);
    (on * (tail + (1.0 - tail) * heavy), on * heavy)
}

/// How much rain a cell's shaft carries (clouds.wgsl `storm_rain`): once its
/// tower is a few km deep; a shower's light to moderate, an ordinary storm's
/// heavy, a big mature one's the heaviest.
fn shaft_amount(a: &At) -> f64 {
    smoothstep(1800.0, 3500.0, a.htop) * smoothstep(800.0, 2000.0, a.r) * (0.35 + 0.4 * smoothstep(0.45, 0.8, a.age) + 0.25 * smoothstep(3000.0, 9000.0, a.r))
}

/// The storm cells' rain, cloud and air at a point (planet frame) `agl` metres
/// above the ground, at game time `t`.
pub fn local(wx: &Weather, pos: DVec3, agl: f64, t: f64) -> Local {
    let mut o = Local::default();
    if wx.cells.is_empty() {
        return o;
    }
    let w = downwind(wx, pos);
    let up = pos.normalize();
    for (i, c) in wx.cells.iter().enumerate() {
        let a = cell_at(wx, c, pos, w);
        let r = a.r;
        let line = a.len2 > 0.0;
        // Under (and in) the anvil streaming downwind: a light chop.
        let anvil = anvil_lid(&a) / 0.55;
        if a.dw0.hypot(a.cx) > r * if line { 5.0 } else { 3.2 } || a.hb > a.htop + 2000.0 {
            if a.hb < a.htop + 2000.0 {
                o.sigma = o.sigma.max(0.7 * anvil * (1.0 + 0.5 * a.sup));
            }
            continue;
        }
        let mature = smoothstep(0.5, 0.8, a.age);
        let size = smoothstep(2200.0, 8500.0, r);
        let amt = shaft_amount(&a);
        // The rain shaft hangs under the downwind half of the base. Along a
        // squall line: a curtain right behind the shelf cloud, a long tail of
        // lighter rain behind that.
        let sup = a.sup;
        let (sa, sx, sr) = shaft_at(sup);
        let rs = (a.dw0 - sa * r).hypot(a.cx - sx * r) / (sr * r);
        let u = shelf_u(&a);
        let ends = 1.0 - smoothstep(a.len2 - 0.3 * r, a.len2 + 0.7 * r, a.cxl.abs());
        let (lr, lh) = line_rain(u, r);
        // (A supercell's core is denser and sharp-edged.)
        let heavy = if line { lr * ends } else { 1.0 - smoothstep(0.35 + 0.27 * sup, 1.0, rs) };
        let fringe = if line { heavy } else { 1.0 - smoothstep(0.85 + 0.15 * sup, 1.45 - 0.3 * sup, rs) };
        let amt = (amt * (1.0 + 0.8 * sup)).min(1.0);
        // The tower, leaning downwind with height.
        let hb = a.hb.max(0.0);
        let hc = (hb / a.htop.max(1.0)).min(1.2);
        let dw = a.dw0 - hb * (0.12 + 0.1 * mature + 0.08 * sup);
        let rt = (0.92 - 0.12 * sup) * r * (1.06 - 0.26 * smoothstep(0.0, 0.88, hc));
        let rr = dw.hypot(a.cx) / rt;
        let inside = if a.hb >= 0.0 && a.hb < a.htop { 1.0 - smoothstep(0.85, 1.08, rr) } else { 0.0 };
        o.cloud = o.cloud.max(inside);
        let rain = if a.hb < 0.0 {
            amt * heavy.max(0.3 * fringe)
        } else {
            // In the cloud: rain under the freezing level, graupel and ice above.
            let core = smoothstep(0.3, 0.7, a.age) * (1.0 - smoothstep(0.4, 1.0, rr)) * (1.0 - 0.65 * smoothstep(0.35, 0.6, hc));
            core.max(amt * heavy * (1.0 - smoothstep(0.0, 1500.0, a.hb))) * (if a.hb < a.htop { 1.0 } else { 0.0 })
        };
        o.rain = o.rain.max(rain);
        // Hail in a supercell's core: under the base and up through the
        // lower half of the tower.
        if sup > 0.0 && a.hb < 0.6 * a.htop {
            o.hail = o.hail.max(sup * (1.0 - smoothstep(0.3, 0.75, rs)));
        }

        // Drafts: the updraft rises through the core (inflow from under the
        // base upwind), strongest halfway up; the downdraft comes down in the
        // rain and spreads out over the ground as the gust front.
        let ru = (dw + 0.25 * r).hypot(a.cx) / (0.6 * r);
        let core = (-ru * ru).exp();
        let up_prof = if a.hb < 0.0 { 0.15 + 0.35 * smoothstep(-1500.0, 0.0, a.hb) } else { (1.0 - smoothstep(0.55, 1.05, hc)) * (0.75 + 0.5 * smoothstep(0.0, 0.5, hc)) };
        let shaft = if line { lh * ends } else { (-(rs / 0.85).powi(2)).exp() };
        let dn_prof = if a.hb < 0.0 { smoothstep(0.0, 300.0, agl) } else { 1.0 - smoothstep(0.0, 0.5, hc) };
        // (Even a young shower tower has a brisk updraft and some bumps.)
        let vigour = 0.35 + 0.65 * a.age.min(1.0);
        // (A supercell's rotating updraft is the strongest of all: 30-40 m/s
        // in its core.)
        o.draft += vigour * ((7.0 + 13.0 * size + 12.0 * sup) * core * up_prof) - amt * (5.0 + 7.0 * size + 6.0 * sup) * shaft * dn_prof;
        // A squall line's gust front: the cold outflow undercuts the warm air
        // ahead and lifts it, a narrow band of updraft under the shelf's nose,
        // with severe turbulence where the two meet.
        let front = if line { ends * mature * (-((u - 300.0) / (0.3 * SHELF_W * r)).powi(2)).exp() } else { 0.0 };
        if line && a.hb < 300.0 {
            o.draft += (5.0 + 4.0 * size) * front * smoothstep(0.0, 250.0, agl);
        }
        // Turbulence: how rough this cell's air gets at its worst (sigma, m/s)
        // goes with how deep and big and vigorous it is: a young shower a
        // couple of m/s (light to moderate), an ordinary thunderstorm 5-6
        // (moderate to severe), a big severe storm or a squall line 7-8
        // (severe), a supercell's rotating updraft 10 and more (extreme).
        // Where in it: worst in the cloud and where the updraft meets the
        // downdraft, in the heavy rain and along the gust front low down;
        // around the cell it builds gradually, and the anvil overhead brings
        // a light chop. It comes in bursts.
        let deep = smoothstep(3000.0, 10500.0, a.htop);
        let peak = vigour * (3.4 + 1.6 * deep + 2.2 * size + 3.5 * sup);
        let shear = (core * shaft * 4.0).min(1.0);
        let near = 1.0 - smoothstep(1.0, 3.0, a.dw0.hypot(a.cx) / r);
        let low_front = if a.hb < 300.0 { front } else { front * (1.0 - smoothstep(300.0, 1500.0, a.hb)) };
        let wet = (amt * heavy * if line { 0.5 + 0.5 * lh } else { 1.0 }).min(1.0);
        // (a lone cell's outflow, low down ahead of its rain)
        let ring = if !line && a.hb < 0.0 {
            let d = (a.dw0 - sa * r).hypot(a.cx - sx * r);
            amt * (d / (0.8 * r)) * (-(d / (1.3 * r)).powi(2)).exp() * 1.4 * (1.0 - smoothstep(150.0, 900.0, agl))
        } else {
            0.0
        };
        let calm = (1.0 - inside) * (1.0 - 0.6 * wet) * (1.0 - shear) * (1.0 - 0.75 * low_front.min(1.0)) * (1.0 - 0.6 * ring.min(1.0)) * (1.0 - 0.22 * near) * (1.0 - 0.12 * anvil);
        let s = peak * (1.0 - calm);
        let k = i as f64 * 1.731;
        let burst = ((0.21 * t + k).sin() * 0.6 + (0.53 * t + 2.3 * k).sin() * 0.4 + (1.37 * t + 4.1 * k).sin() * 0.25) / 1.25;
        o.sigma = o.sigma.max(s * (0.7 + 0.45 * smoothstep(-0.2, 0.7, burst)));
        // Gust front: the downdraft fans out from under the shaft, low down.
        // Under a squall line it rushes forward all along it: a sudden wind
        // shift as the shelf passes over, the outflow blowing on through the
        // rain behind (and ahead, the warm air drawn in towards the line).
        if line && a.hb < 0.0 {
            let low = 1.0 - smoothstep(150.0, 1000.0, agl);
            let out = smoothstep(-200.0, 600.0, u) * (1.0 - smoothstep(SHELF_W * r + 1.5 * r, SHELF_W * r + 3.5 * r, u));
            let inflow = (1.0 - smoothstep(-200.0, 400.0, u)) * (-(u / (2.0 * r)).powi(2)).exp();
            let fw = (a.w - up * a.w.dot(up)).normalize_or_zero();
            o.gust += fw * (ends * mature * low * (amt * (10.0 + 9.0 * size) * out - 3.0 * inflow));
        } else if a.hb < 0.0 && amt > 0.0 {
            let out = a.w * (a.dw0 - sa * r) + a.x * (a.cx - sx * r);
            let d = out.length();
            if d > 1.0 {
                let ring = (d / (0.8 * r)) * (-(d / (1.3 * r)).powi(2)).exp() * 1.6;
                let low = 1.0 - smoothstep(150.0, 900.0, agl);
                let dir = (out - up * out.dot(up)) / d;
                o.gust += dir * (amt * (6.0 + 10.0 * size) * ring * low);
            }
        }
    }
    o.rain = o.rain.min(1.0);
    o
}

/// A horizontal bounding circle holding all of storm cell `c`: xyz centre on
/// the planet surface (planet frame, m) and w radius (m). Precomputed once per
/// frame on the CPU for Globals.storm_bounds (common.wgsl `storm_bound`).
pub fn bound(c: &[[f32; 4]; 2], cloud_off: DVec3, wind_dir: DVec3, base: f32) -> [f32; 4] {
    let r = c[0][3] as f64 * 1000.0;
    let age = c[1][0] as f64;
    let mature = smoothstep(0.5, 0.8, age);
    let sup = crate::weather::supercell(c);
    let line = c[1][3] >= 1.0;
    let reach = if line { 2.6 } else { 4.0 };
    let ax = r * (1.0 + (reach + (7.8 - reach) * sup) * mature) * 1.22;
    let ay = r * (0.9 + 0.35 * mature + 0.35 * sup) * 1.22;
    let az = r * (1.1 + 0.65 * mature + 0.25 * sup) * 1.22;
    let lean = (0.22 + 0.08 * sup) * (c[1][1] as f64 - base as f64).max(0.0);
    let up = (DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64) - cloud_off * 0.001).normalize();
    let cen = up * geo::R + wind_dir * (0.5 * (ax - ay) + lean * 0.5);
    let bulge = (r * 0.25).clamp(1000.0, 3200.0);
    let rb = (0.5 * (ax + ay) + lean * 0.5).max(az + ax * 0.45).max(0.5 * (ax - ay) + lean * 0.5 + r * 4.2) + bulge + 2500.0;
    [cen.x as f32, cen.y as f32, cen.z as f32, rb as f32]
}

/// Precomputed lone-cell turrets (slots 0..5: `[tc.x, tc.y, rk, ck]`) and
/// flanking towers (slots 5..8: `[fo.x, fo.y, fr, ft]`) for `Globals.storm_turrets`
/// (common.wgsl `storm_sd`), avoiding repeated 4D hashes and trig on every ray step.
pub fn turrets(c: &[[f32; 4]; 2], base: f32) -> [[f32; 4]; 8] {
    let mut out = [[0.0f32; 4]; 8];
    let r = c[0][3] * 1000.0;
    let sp = c[1];
    let l2 = sp[3].floor() * 500.0;
    if r <= 0.0 || l2 > 0.0 {
        return out;
    }
    let seed = sp[3] - sp[3].floor();
    let mature = smoothstep(0.5, 0.8, sp[0] as f64) as f32;
    let sup = (sp[0] - 1.0).clamp(0.0, 1.0);
    let at = sp[1] - base;
    let htop = at * storm_top(sp[0] as f64) as f32;
    let hash4 = |x: f32| -> [f32; 4] {
        let f = |v: f32| v - v.floor();
        let mut p = [f(x * 0.1031), f(x * 0.1030), f(x * 0.0973), f(x * 0.1099)];
        let d = p[0] * (p[3] + 33.33) + p[1] * (p[2] + 33.33) + p[2] * (p[0] + 33.33) + p[3] * (p[1] + 33.33);
        for v in &mut p {
            *v += d;
        }
        [f((p[0] + p[1]) * p[2]), f((p[0] + p[2]) * p[1]), f((p[1] + p[2]) * p[3]), f((p[2] + p[3]) * p[0])]
    };
    if sup <= 0.5 {
        let a0 = seed * 17.0;
        let mut tdir = [a0.cos(), a0.sin()];
        for (k, slot) in out[..5].iter_mut().enumerate() {
            let fk = k as f32;
            let hs = hash4(seed * 91.7 + fk * 12.989);
            let rk = r * (0.48 + 0.22 * hs[2]);
            let ft = htop * (0.46 + (0.92 - 0.46) * hs[3]) * (0.80 + 0.20 * mature);
            let dj = 0.4775 * hs[0];
            let dj2 = dj * dj;
            let cd = 1.0 - 0.5 * dj2 + 0.041667 * dj2 * dj2;
            let sd_ = dj * (1.0 - 0.166667 * dj2);
            let dist = r * (0.58 + 0.30 * hs[1]);
            let tc = [(tdir[0] * cd - tdir[1] * sd_) * dist, (tdir[0] * sd_ + tdir[1] * cd) * dist];
            tdir = [tdir[0] * 0.309017 - tdir[1] * 0.9510565, tdir[0] * 0.9510565 + tdir[1] * 0.309017];
            let ck = (ft - rk).max(0.0);
            *slot = [tc[0], tc[1], rk, ck];
        }
    }
    let fl = [-0.75f32, -0.66f32];
    for k in 1..=3 {
        let fk = k as f32;
        let h = hash4(seed * 53.1 + fk * 17.37);
        let fh4 = [h[0] * 2.0 - 1.0, h[1] * 2.0 - 1.0, h[2] * 2.0 - 1.0, h[3] * 2.0 - 1.0];
        let a = r * (0.52 + 0.72 * fk + 0.12 * fh4[0]);
        let fo = [fl[0] * a + fh4[1] * (r * 0.28), fl[1] * a + fh4[2] * (r * 0.28)];
        let fr = r * (0.72 - 0.14 * fk) * (1.0 - 0.35 * sup);
        let ft = htop * (0.74 - 0.16 * fk) * (0.65 + 0.35 * mature) * (0.9 + 0.18 * fh4[3]);
        out[4 + k] = [fo[0], fo[1], fr, ft];
    }
    out
}

/// The storm cells (of those handed to the shaders) close enough to the camera
/// to darken the ground in view, as a bit mask (Globals.stormfx.x).
pub fn near_mask(wx: &Weather, cam: DVec3) -> u32 {
    let mut m = 0;
    for (i, c) in wx.cells.iter().enumerate().take(24) {
        let cen = DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64) * 1000.0 - wx.cloud_off;
        let d = (cen.normalize() - cam.normalize()).length() * geo::R;
        if d < c[0][3] as f64 * 1000.0 * 6.0 + 50_000.0 + line_of(c).0 {
            m |= 1 << i;
        }
    }
    m
}

/// How much of the anvil's thick grey lid is overhead, 0..0.55 (its shade).
fn anvil_lid(a: &At) -> f64 {
    let r = a.r;
    let mature = smoothstep(0.5, 0.8, a.age);
    let sup = a.sup;
    let reach = if a.len2 > 0.0 { 2.6 } else { 4.0 };
    let an = (r * (1.0 + (reach + (7.8 - reach) * sup) * mature), r * (0.9 + 0.35 * mature + 0.35 * sup), r * (1.1 + 0.65 * mature + 0.25 * sup));
    let lc = an.2 + a.dw0.max(0.0) * (0.42 - 0.14 * sup);
    let rn = (a.dw0 / if a.dw0 > 0.0 { an.0 } else { an.1 }).hypot(a.cx / lc);
    (1.0 - smoothstep(0.5, 1.05, rn)) * 0.55 * mature
}

/// How deep in a storm's shade a point is, 0..1: under the tower and its rain
/// nearly all the sky is dark cloud; under the anvil a thick grey lid. Must
/// match common.wgsl `storm_shade`.
pub fn shade(wx: &Weather, pos: DVec3) -> f64 {
    let w = downwind(wx, pos);
    let mut dark = 0.0f64;
    for c in &wx.cells {
        let a = cell_at(wx, c, pos, w);
        let r = a.r;
        let rc = (a.dw0 - 0.25 * r).hypot(a.cx) / r;
        let core = (1.0 - smoothstep(0.7, 1.7, rc)) * smoothstep(0.3, 0.75, a.age) * smoothstep(1500.0, 4000.0, r);
        dark = dark.max(core.max(anvil_lid(&a)));
    }
    let alt = pos.length() - geo::R;
    dark * (1.0 - smoothstep(wx.base() as f64, wx.base() as f64 + 3000.0, alt))
}

/// Lightning that stays in the cloud, or comes down to the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Cloud,
    Ground,
}

/// A straight piece of a lightning channel.
#[derive(Clone, Copy, Debug)]
struct Seg {
    a: DVec3,
    b: DVec3,
    /// drawn width, m
    w: f32,
    /// 0 the main channel, 1 a branch, 2 a branch of a branch
    level: u8,
    /// when the leader reaches it, 0..1 of the leader's time
    t: f32,
    /// brightness along it: branches fade out towards their tips
    f: f32,
}

/// One flash: its channel and its strokes.
struct Flash {
    kind: Kind,
    /// seconds since the first return stroke (negative while the leader comes down)
    age: f64,
    leader: f64,
    /// (time after the first stroke, strength)
    strokes: Vec<(f64, f64)>,
    /// for each stroke, the stretch of channel it lights (planet frame)
    spans: Vec<[DVec3; 2]>,
    segs: Vec<Seg>,
    power: f64,
    end: f64,
    /// how brightly an in-cloud flash's channel shows (anvil crawlers, under
    /// the thin ice, the most)
    vis: f64,
}

/// A thunderclap on its way: how long until it arrives, the nearest and
/// farthest points of its channel (m), how strong, and where it comes from.
#[derive(Clone, Copy, Debug)]
pub struct Thunder {
    pub delay: f64,
    pub near: f64,
    pub far: f64,
    pub power: f64,
    pub from: DVec3,
}

/// A strike forced by the command line (`--strike`): game time of its first
/// return stroke, kind, distance (km) and compass bearing from the aircraft.
#[derive(Clone, Copy, Debug)]
pub struct Forced {
    pub at: f64,
    pub kind: Kind,
    pub km: f64,
    pub bearing: f64,
}

pub struct Lightning {
    flashes: Vec<Flash>,
    rng: u64,
    pub forced: Vec<Forced>,
    /// thunder set off since last taken (main.rs hands it to the sound)
    pub thunder: Vec<Thunder>,
}

/// Brightness of one stroke `d` seconds after it struck: the stroke itself,
/// then the glow of the current still flowing. The frame it strikes in shows
/// it at full strength (a camera's exposure would catch the whole flash).
fn stroke(d: f64, dt: f64) -> f64 {
    if d < 0.0 {
        return 0.0;
    }
    let e = (d - dt).max(0.0);
    (-e / 0.035).exp() + 0.22 * (-e / 0.16).exp()
}

impl Default for Lightning {
    fn default() -> Self {
        Self::new()
    }
}

impl Lightning {
    pub fn new() -> Self {
        Self { flashes: Vec::new(), rng: 0x9e37_79b9_7f4a_7c15, forced: Vec::new(), thunder: Vec::new() }
    }

    fn u(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.u()
    }
    fn unit(&mut self) -> DVec3 {
        loop {
            let v = DVec3::new(self.u(), self.u(), self.u()) * 2.0 - 1.0;
            let l = v.length_squared();
            if l > 1e-4 && l <= 1.0 {
                return v / l.sqrt();
            }
        }
    }

    /// A tortuous channel from a to b by midpoint displacement: `levels`
    /// halvings, each new point pushed sideways by up to `rough` of its piece's
    /// length on the coarse levels and much less on the fine ones, so a bolt is
    /// a few long, crooked strokes with medium kinks, not a fizzy squiggle at
    /// every scale (equal roughness per level made a self-similar fractal that
    /// read as electric fuzz). Branches (`fade`) dim towards their tips.
    fn channel(&mut self, a: DVec3, b: DVec3, levels: u32, rough: f64, w: f32, level: u8, t0: f32, t1: f32, fade: bool, out: &mut Vec<Seg>) -> Vec<DVec3> {
        const PROFILE: [f64; 8] = [1.6, 1.45, 1.35, 1.25, 0.9, 0.42, 0.18, 0.1];
        let mut pts = vec![a, b];
        let axis = (b - a).normalize_or_zero();
        for l in 0..levels {
            let mut next = Vec::with_capacity(pts.len() * 2);
            let k = rough * PROFILE[(l as usize).min(7)];
            for i in 0..pts.len() - 1 {
                let (p, q) = (pts[i], pts[i + 1]);
                let len = p.distance(q);
                let mut off = self.unit();
                off -= axis * off.dot(axis) * 0.7;
                // Kinks off-centre, so the pieces are of uneven length.
                let m = self.range(0.3, 0.7);
                next.push(p);
                next.push(p + (q - p) * m + off * len * k);
            }
            next.push(*pts.last().unwrap());
            pts = next;
        }
        let n = pts.len() - 1;
        for i in 0..n {
            let t = t0 + (t1 - t0) * (i as f32 / n as f32);
            let f = if fade { 1.0 - 0.85 * ((i as f32 + 0.5) / n as f32).powf(0.8) } else { 1.0 };
            out.push(Seg { a: pts[i], b: pts[i + 1], w, level, t, f });
        }
        pts
    }

    /// Side branches off a channel's points: forking down and out, shorter
    /// lower down, some forking again.
    fn branches(&mut self, pts: &[DVec3], up: DVec3, ground: f64, count: usize, level: u8, t0: f32, t1: f32, out: &mut Vec<Seg>) {
        let n = pts.len();
        if n < 4 {
            return;
        }
        for _ in 0..count {
            let k = ((self.u().powf(1.3) * 0.8 + 0.02) * (n - 2) as f64) as usize;
            let p = pts[k];
            let dir = (pts[(k + 1).min(n - 1)] - p).normalize_or_zero();
            let h = (p.length() - geo::R - ground).max(50.0);
            let len = h * self.range(0.25, 0.6) / (1.0 + level as f64);
            let side = self.unit();
            let mut d = (dir * 0.6 + (side - up * side.dot(up)).normalize_or_zero() * 0.8 - up * 0.35).normalize_or_zero();
            if level == 1 && self.u() < 0.3 {
                d = (d + up * 0.3).normalize_or_zero();
            }
            let tb = t0 + (t1 - t0) * (k as f32 / n as f32);
            let w = if level == 1 { 0.45 } else { 0.25 };
            let lv = if level == 1 { 5 } else { 4 };
            let bp = self.channel(p, p + d * len, lv, 0.2, w, level, tb, (tb + 0.5 * (t1 - tb)).min(t1), true, out);
            if level == 1 && self.u() < 0.35 {
                let nb = 1;
                self.branches(&bp, up, ground, nb, 2, tb, t1, out);
            }
        }
    }

    /// A cloud-to-ground flash from `top` (in the cloud) down to `foot` on the ground.
    /// `positive`: a positive strike out of the anvil, one stroke with a long
    /// continuing current (re-brightening twice) and a long in-cloud root.
    fn ground_flash(&mut self, top: DVec3, foot: DVec3, ground: f64, power: f64, forced: bool, positive: bool) -> Flash {
        let up = foot.normalize();
        let mut segs = Vec::new();
        // The part in the cloud runs off sideways from where the leader started.
        let h = top.distance(foot);
        let pts = self.channel(top, foot, 7, 0.17, 1.0, 0, 0.0, 1.0, false, &mut segs);
        let nb = 2 + (self.u() * 3.5) as usize;
        self.branches(&pts, up, ground, nb, 1, 0.0, 1.0, &mut segs);
        let side = self.unit();
        let root = top + (side - up * side.dot(up)).normalize_or_zero() * h * if positive { 1.3 } else { 0.35 } + up * h * 0.08;
        self.channel(top, root, if positive { 6 } else { 5 }, 0.22, 0.7, 1, 0.0, 0.0, true, &mut segs);
        if positive {
            let spans = vec![[top, foot]; 3];
            return Flash { kind: Kind::Ground, age: 0.0, leader: self.range(0.03, 0.05), end: 1.2, strokes: vec![(0.0, 1.0), (0.14, 0.4), (0.38, 0.25)], spans, segs, power, vis: 1.0 };
        }
        // Strokes: the first and strongest, then up to four more down the same channel.
        let mut strokes = vec![(0.0, 1.0)];
        let n = if forced { 4 } else { 1 + (self.u().powf(0.8) * 4.0) as usize };
        let mut t = 0.0;
        for _ in 1..n {
            t += if forced { 0.075 } else { self.range(0.035, 0.12) };
            let a = if forced { 0.7 } else { self.range(0.35, 0.85) };
            strokes.push((t, a));
        }
        let spans = vec![[top, foot]; strokes.len()];
        Flash { kind: Kind::Ground, age: 0.0, leader: if forced { 0.03 } else { self.range(0.02, 0.035) }, end: t + 0.5, strokes, spans, segs, power, vis: 1.0 }
    }

    /// An in-cloud flash around `c`: a few pulses, each lighting another
    /// stretch of the cloud, and a faint channel that shows only where the
    /// cloud is thin.
    fn cloud_flash(&mut self, c: DVec3, spread: f64, power: f64, forced: bool) -> Flash {
        let up = c.normalize();
        let flat = |v: DVec3| (v - up * v.dot(up) * 0.75).normalize_or_zero();
        let n = if forced { 5 } else { 3 + (self.u() * 6.0) as usize };
        let mut strokes = Vec::new();
        let mut spans = Vec::new();
        let mut t = 0.0;
        for k in 0..n {
            let a = if forced && k == 0 { 1.0 } else { self.range(0.25, 1.0) };
            strokes.push((t, a));
            let o = flat(self.unit()) * spread * self.range(0.0, 0.6);
            let d = flat(self.unit()) * spread * self.range(0.3, 0.9);
            spans.push([c + o - d, c + o + d]);
            t += self.range(0.04, 0.16);
        }
        let mut segs = Vec::new();
        let [a, b] = spans[0];
        let pts = self.channel(a, b, 6, 0.2, 0.7, 0, 0.0, 0.0, false, &mut segs);
        let nb = 2 + (self.u() * 4.0) as usize;
        for _ in 0..nb {
            let p = pts[(self.u() * (pts.len() - 1) as f64) as usize];
            let d = flat(self.unit()) * spread * self.range(0.2, 0.6);
            self.channel(p, p + d, 4, 0.22, 0.4, 1, 0.0, 0.0, true, &mut segs);
        }
        Flash { kind: Kind::Cloud, age: 0.0, leader: 0.0, end: t + 0.3, strokes, spans, segs, power, vis: 1.0 }
    }

    /// Advance by `dt` game seconds at game time `time`: let the cells flash,
    /// set off forced strikes, retire finished flashes. `cam` is where the
    /// thunder is heard; `ground(p)` the height of the ground under p (m).
    pub fn step(&mut self, dt: f64, time: f64, wx: &Weather, plane: DVec3, cam: DVec3, ground: &dyn Fn(DVec3) -> f64) {
        if dt <= 0.0 {
            return;
        }
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < f.end);
        let base = wx.base() as f64;
        // Forced strikes begin their leader just ahead of the stroke time.
        let mut i = 0;
        while i < self.forced.len() {
            let fs = self.forced[i];
            if time + 0.04 < fs.at {
                i += 1;
                continue;
            }
            self.forced.swap_remove(i);
            let (e, n, up) = geo::enu(plane);
            let b = fs.bearing.to_radians();
            let dir = e * b.sin() + n * b.cos();
            let a = fs.km * 1000.0 / geo::R;
            let un = (up * a.cos() + dir * a.sin()).normalize();
            let g = ground(un * geo::R).max(0.0);
            let mut f = match fs.kind {
                Kind::Ground => {
                    let side = self.unit();
                    let top = un * (geo::R + base + 3200.0) + (side - un * side.dot(un)) * 500.0;
                    self.ground_flash(top, un * (geo::R + g), g, 1.2, true, false)
                }
                Kind::Cloud => self.cloud_flash(un * (geo::R + base + 4500.0), 3500.0, 1.0, true),
            };
            f.age = time - fs.at;
            self.sound(&f, cam);
            self.flashes.push(f);
        }
        // Each cell flashes at random, more often the bigger and more mature it is.
        let w = downwind(wx, cam);
        for ci in 0..wx.cells.len() {
            let c = wx.cells[ci];
            let r = c[0][3] as f64 * 1000.0;
            let age = c[1][0] as f64;
            let tall = smoothstep(LIGHTNING_TOP - 800.0, LIGHTNING_TOP + 800.0, c[1][1] as f64 - (c[1][1] as f64 - base) * (1.0 - storm_top(age)));
            let strength = tall * smoothstep(0.5, 0.9, age) * (0.12 + 0.88 * smoothstep(2200.0, 8500.0, r)) * (0.5 + 0.5 * wx.storms.max(0.6) as f64);
            // A big mature cell: ~10 flashes a minute; an ordinary one, one or two.
            // A squall line flashes all along its length, like a row of them.
            let (len2, _) = line_of(&c);
            // A supercell: near-continuous, a flash every second or so.
            let sup = crate::weather::supercell(&c);
            let rate = strength * 10.0 / 60.0 * (1.0 + 0.7 * len2 / r) * (1.0 + 5.0 * sup);
            if rate <= 0.0 || self.flashes.len() >= 12 || self.u() >= rate * dt {
                continue;
            }
            let cen = DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64) * 1000.0 - wx.cloud_off;
            let un = cen.normalize();
            let x = un.cross(w).normalize_or_zero();
            let at = c[1][1] as f64 - base;
            let htop = at * storm_top(age);
            let power = self.range(0.6, 1.3) * (0.6 + 0.4 * strength);
            let (sa, sx, sr) = shaft_at(sup);
            let f = if self.u() < 0.3 - 0.08 * sup {
                // Down to the ground, mostly in and around the rain shaft;
                // now and then a stronger one out from under the anvil (from
                // a supercell's, often, and far out: positive strikes tens
                // of km from the core, out of a clear sky).
                let out = self.u() < 0.08 + 0.25 * sup;
                let far = out && sup > 0.0;
                let (along, mut across) = if far {
                    (self.range(1.6, 5.5) * r, self.range(-1.0, 1.0) * r)
                } else if out {
                    (self.range(1.4, 2.6) * r, self.range(-0.8, 0.8) * r)
                } else if len2 > 0.0 {
                    ((SHELF_F - SHELF_W - self.range(0.1, 1.0)) * r, 0.0)
                } else {
                    (sa * r + self.range(-0.9, 0.9) * r * sr, sx * r + self.range(-0.9, 0.9) * r * 0.94 * sr)
                };
                across += self.range(-len2, len2);
                let foot_dir = (un * geo::R + w * along + x * across).normalize();
                let g = ground(foot_dir * geo::R).max(0.0);
                let hstart = if far { at - self.range(1200.0, 2200.0) } else if out { at - 2500.0 } else { self.range(2000.0, 4500.0).min(htop * 0.6) };
                let lean = hstart * 0.15;
                let top = (un * geo::R + w * (along * if far { 0.85 } else { 0.4 } + lean) + x * across * 0.4).normalize() * (geo::R + base + hstart);
                self.ground_flash(top, foot_dir * (geo::R + g), g, power * if far { 2.5 } else if out { 2.0 } else { 1.0 }, false, far)
            } else {
                // In the cloud: mostly in the middle of the tower, sometimes out
                // in the anvil. A supercell's flicker all through its tower,
                // and anvil crawlers: long branching channels spreading for
                // tens of km just under the anvil's thin underside, far
                // downwind of the core, the channels showing through the ice.
                let anvil = age > 0.7 && self.u() < 0.3;
                let crawl = anvil && sup > 0.0 && self.u() < 0.7 * sup;
                let (along, h) = if crawl {
                    (self.range(1.5, 7.5) * r, at - self.range(2600.0, 4200.0))
                } else if anvil {
                    (self.range(1.0, 3.5) * r, at - self.range(800.0, 2500.0))
                } else {
                    (self.range(-0.4, 0.5) * r, htop * self.range(0.35 - 0.15 * sup, 0.8))
                };
                let across = self.range(-0.5, 0.5) * r * (1.0 + 2.0 * crawl as u8 as f64) + self.range(-len2, len2);
                let p = (un * geo::R + w * (along + h * (0.15 + 0.1 * sup)) + x * across).normalize() * (geo::R + base + h);
                let spread = if crawl { self.range(7000.0, 16000.0) } else { (0.5 * r).clamp(1500.0, 4500.0) };
                let mut f = self.cloud_flash(p, spread, power * if crawl { 1.2 } else { 0.8 }, false);
                if crawl {
                    f.vis = 2.6;
                }
                f
            };
            let mut f = f;
            f.age = -f.leader;
            self.sound(&f, cam);
            self.flashes.push(f);
        }
    }

    /// Queue the flash's thunder: it starts when the sound from the nearest
    /// bit of channel arrives and rolls on until that from the farthest has.
    fn sound(&mut self, f: &Flash, cam: DVec3) {
        let (mut near, mut far, mut at) = (f64::MAX, 0.0f64, DVec3::ZERO);
        for s in f.segs.iter().filter(|s| s.level == 0) {
            let d = s.a.distance(cam);
            if d < near {
                (near, at) = (d, s.a);
            }
            far = far.max(d);
        }
        if near == f64::MAX || near > 40_000.0 {
            return;
        }
        let power = f.power * if f.kind == Kind::Ground { 1.0 } else { 0.55 };
        self.thunder.push(Thunder { delay: near / SOUND + (-f.age).max(0.0), near, far, power, from: (at - cam).normalize_or_zero() });
    }

    /// Light given off by a flash now (relative to its peak), and which of
    /// its strokes is the latest.
    fn light(f: &Flash, dt: f64) -> (f64, usize) {
        let mut b = 0.0;
        let mut last = 0;
        for (k, &(ts, amp)) in f.strokes.iter().enumerate() {
            if f.age >= ts {
                last = k;
            }
            b += amp * stroke(f.age - ts, dt);
        }
        (b * f.power, last)
    }

    /// The flashes lighting the scene this frame, brightest (as seen from the
    /// camera) first: per flash, the two ends of the lit stretch (camera-relative)
    /// + brightness, and the kind. Returns how many.
    pub fn globals(&self, cam: DVec3, dt: f64, out: &mut [[f32; 4]; 2 * MAX_FLASHES]) -> usize {
        let mut lit: Vec<(f64, [[f32; 4]; 2])> = Vec::new();
        for f in &self.flashes {
            let (b, k) = Self::light(f, dt);
            if b < 0.004 {
                continue;
            }
            let [a, e] = f.spans[k.min(f.spans.len() - 1)];
            let d = ((a + e) * 0.5).distance(cam);
            let v = |p: DVec3, w: f64| [(p - cam).x as f32, (p - cam).y as f32, (p - cam).z as f32, w as f32];
            lit.push((b / (1.0 + (d / 20000.0).powi(2)), [v(a, b), v(e, if f.kind == Kind::Ground { 1.0 } else { 0.0 })]));
        }
        lit.sort_by(|x, y| y.0.total_cmp(&x.0));
        for (i, (_, l)) in lit.iter().take(MAX_FLASHES).enumerate() {
            out[2 * i] = l[0];
            out[2 * i + 1] = l[1];
        }
        lit.len().min(MAX_FLASHES)
    }

    /// The channels to draw this frame, camera-relative: per segment one end +
    /// width (m), the other end + brightness (precip.wgsl `vs_bolt`).
    pub fn segments(&self, cam: DVec3, dt: f64, out: &mut Vec<[f32; 8]>) {
        for f in &self.flashes {
            let mut main = 0.0;
            let mut side = 0.0;
            for (k, &(ts, amp)) in f.strokes.iter().enumerate() {
                let s = amp * stroke(f.age - ts, dt);
                main += s;
                // Only the first stroke lights the branches; later ones follow the main channel.
                side += s * if k == 0 { 1.0 } else { 0.08 };
            }
            // The stepped leader: a faint, branching feeler working its way down.
            let lead = if f.age < 0.0 && f.leader > 0.0 { Some(((f.age + f.leader) / f.leader) as f32) } else { None };
            let kind = if f.kind == Kind::Ground { 1.0 } else { 0.3 * f.vis };
            for s in &f.segs {
                let b = match lead {
                    Some(p) if s.t <= p => 0.05 * (0.3 + 0.7 * ((s.t - p + 0.15) / 0.15).clamp(0.0, 1.0)) as f64,
                    Some(_) => 0.0,
                    None => if s.level == 0 { main } else { side * if s.level == 1 { 0.55 } else { 0.35 } },
                } * f.power.min(1.5)
                    * kind
                    * s.f as f64;
                if b < 0.002 || out.len() >= MAX_SEGS {
                    continue;
                }
                let (a, e) = (s.a - cam, s.b - cam);
                out.push([a.x as f32, a.y as f32, a.z as f32, s.w, e.x as f32, e.y as f32, e.z as f32, b as f32]);
            }
        }
    }

}

/// The rain falling around the camera: how far it has moved past the camera
/// (wrapped), for precip.wgsl's drops to keep their places from frame to frame.
#[derive(Default)]
pub struct RainField {
    /// metres along east, north, up (wrapped every `WRAP` m)
    pub off: DVec3,
    prev_cam: Option<DVec3>,
    /// the rain's velocity relative to the camera, planet frame, m/s
    pub rel: DVec3,
}

/// Every drop box size (precip.wgsl RAIN_BOX) divides this, metres.
const WRAP: f64 = 200.0;
/// How fast a typical raindrop falls, m/s.
const FALL: f64 = 7.5;
/// How fast a snowflake falls, m/s (aggregates of a few millimetres fall at
/// 0.5-1.5 m/s, Locatelli and Hobbs 1974).
pub const FLAKE_FALL: f64 = 1.0;

impl RainField {
    /// Advance by `dt` with the camera at `cam` and the wind there `wind` (planet frame).
    pub fn step(&mut self, dt: f64, cam: DVec3, wind: DVec3) {
        self.step_fall(dt, cam, wind, FALL);
    }

    /// The same for what falls at `fall` m/s (snowflakes: `FLAKE_FALL`).
    pub fn step_fall(&mut self, dt: f64, cam: DVec3, wind: DVec3, fall: f64) {
        let v_cam = match self.prev_cam {
            Some(p) if dt > 0.0 => {
                let v = (cam - p) / dt;
                if v.length() < 500.0 { v } else { DVec3::ZERO }
            }
            _ => DVec3::ZERO,
        };
        self.prev_cam = Some(cam);
        if dt <= 0.0 {
            return;
        }
        let (e, n, u) = geo::enu(cam);
        let rel = wind - u * fall - v_cam;
        // Smoothed a little: the chase camera's spring jitters frame to frame.
        self.rel += (rel - self.rel) * (dt * 20.0).min(1.0);
        let d = self.rel * dt;
        self.off = DVec3::new((self.off.x + d.dot(e)).rem_euclid(WRAP), (self.off.y + d.dot(n)).rem_euclid(WRAP), (self.off.z + d.dot(u)).rem_euclid(WRAP));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storm() -> (Weather, DVec3) {
        let mut wx = Weather::new(0.4);
        wx.storms = 0.0;
        (wx.wind, wx.wind_from) = (8.0, 270.0);
        let p = geo::lla_to_pos(0.65, -2.14, 600.0);
        wx.place_storm(p, 0.0, 0.0, 6.0, None);
        wx.cells = wx.storm_cells(p);
        (wx, p)
    }

    #[test]
    fn rain_under_the_shaft_not_in_clear_air() {
        let (wx, p) = storm();
        let (e, _, u) = geo::enu(p);
        let w = downwind(&wx, p);
        assert!(w.dot(e) > 0.9, "a westerly blows east");
        // Under the downwind half of the base: heavy rain and a downdraft.
        let under = (p + e * 2700.0).normalize() * (geo::R + 600.0);
        let l = local(&wx, under, 600.0, 0.0);
        assert!(l.rain > 0.8 && l.draft < -3.0 && l.sigma > 2.0, "{l:?}");
        // Out at the edge: light rain.
        let edge = (p + e * 7500.0).normalize() * (geo::R + 600.0);
        let l = local(&wx, edge, 600.0, 0.0);
        assert!(l.rain > 0.02 && l.rain < 0.5, "{l:?}");
        // Far off in clear air: nothing.
        let far = (p + e * 30000.0).normalize() * (geo::R + 600.0);
        let l = local(&wx, far, 600.0, 0.0);
        assert!(l.rain == 0.0 && l.sigma < 1.0, "only a light chop under the anvil: {l:?}");
        let side = (p + geo::enu(p).1 * 30000.0).normalize() * (geo::R + 600.0);
        assert!(local(&wx, side, 600.0, 0.0).sigma == 0.0);
        // In the tower's heart: cloud, updraft.
        let core = (p - e * 1000.0).normalize() * (geo::R + wx.base() as f64 + 3000.0);
        let l = local(&wx, core, 4000.0, 0.0);
        assert!(l.cloud > 0.9 && l.draft > 5.0, "{l:?}");
        let _ = u;
    }

    #[test]
    fn a_shower_rains_and_bumps_but_never_flashes() {
        let mut wx = Weather::new(0.4);
        wx.storms = 0.0;
        (wx.wind, wx.wind_from) = (8.0, 270.0);
        let p = geo::lla_to_pos(0.65, -2.14, 600.0);
        wx.place_storm(p, 0.0, 0.0, 1.8, Some(crate::weather::CellKind::Shower));
        wx.cells = wx.storm_cells(p);
        let (e, _, _) = geo::enu(p);
        // Under its shaft: light to moderate rain, some turbulence.
        let under = (p + e * 800.0).normalize() * (geo::R + 600.0);
        let l = local(&wx, under, 600.0, 0.0);
        assert!(l.rain > 0.2 && l.rain < 0.5 && l.sigma > 0.3 && l.sigma < 2.5, "{l:?}");
        // A few km away: dry.
        let l = local(&wx, (p + e * 5000.0).normalize() * (geo::R + 600.0), 600.0, 0.0);
        assert!(l.rain == 0.0, "{l:?}");
        let mut lt = Lightning::new();
        for i in 0..36000 {
            lt.step(1.0 / 60.0, i as f64 / 60.0, &wx, p, p, &|_| 0.0);
        }
        assert!(lt.thunder.is_empty(), "{} thunderclaps from a shower", lt.thunder.len());
        // An ordinary thunderstorm flashes now and then.
        let mut wx2 = Weather::new(0.4);
        wx2.storms = 0.0;
        wx2.place_storm(p, 0.0, 0.0, 3.5, Some(crate::weather::CellKind::Thunder));
        wx2.cells = wx2.storm_cells(p);
        let mut lt = Lightning::new();
        let mut n = 0;
        for i in 0..36000 {
            lt.step(1.0 / 60.0, i as f64 / 60.0, &wx2, p, p, &|_| 0.0);
            n += lt.thunder.drain(..).count();
        }
        assert!((3..=30).contains(&n), "{n} thunderclaps in 10 minutes from an ordinary storm");
    }

    #[test]
    fn flying_through_a_squall_line() {
        // A squall line 20 km west of p, coming on from the west.
        let mut wx = Weather::new(0.3);
        wx.storms = 0.0;
        (wx.wind, wx.wind_from) = (10.0, 270.0);
        let p = geo::lla_to_pos(0.72, -1.68, 400.0);
        wx.place_squall(p, 20.0, 270.0, 100.0);
        wx.cells = wx.storm_cells(p);
        let (e, _, _) = geo::enu(p);
        let wob = |km: f64| {
            let q = (p - e * (km * 1000.0)).normalize() * (geo::R + 400.0);
            let a = cell_at(&wx, &wx.cells[0], q, downwind(&wx, q));
            (q, shelf_u(&a))
        };
        // Walk west along the line of flight, 400 m up, and note what happens.
        let mut seen = Vec::new();
        for i in 0..400 {
            let km = 10.0 + i as f64 * 0.1;
            let (q, u) = wob(km);
            seen.push((km, u, local(&wx, q, 330.0, 0.0)));
        }
        let at = |u0: f64, u1: f64| seen.iter().filter(move |s| s.1 >= u0 && s.1 < u1).map(|s| s.2);
        // Ahead of it: dry, smooth, a little inflow towards it.
        for l in at(-8000.0, -3000.0) {
            assert!(l.rain == 0.0 && l.sigma < 1.5 && l.draft.abs() < 1.5, "{l:?}");
        }
        // The gust front under the shelf's nose: lift, severe turbulence, and
        // the wind swinging round to blow out ahead of the line.
        let front: Vec<Local> = at(0.0, 800.0).collect();
        assert!(front.iter().any(|l| l.draft > 3.0) && front.iter().all(|l| l.sigma > 3.0 && l.rain == 0.0), "{front:?}");
        let gust_ahead = at(-3000.0, -1500.0).map(|l| l.gust.dot(e)).fold(0.0, f64::min);
        let gust_behind = at(2000.0, 3000.0).map(|l| l.gust.dot(e)).fold(0.0, f64::max);
        assert!(gust_ahead <= 0.0 && gust_behind > 10.0, "{gust_ahead} {gust_behind}");
        // Behind the shelf: heavy rain coming down in a downdraft...
        let core: Vec<Local> = at(5500.0, 8000.0).collect();
        assert!(!core.is_empty() && core.iter().all(|l| l.rain > 0.8 && l.draft < -3.0), "{core:?}");
        // ...then lighter, steady rain easing off far behind.
        let tail: Vec<Local> = at(14000.0, 20000.0).collect();
        assert!(!tail.is_empty() && tail.iter().all(|l| l.rain > 0.05 && l.rain < 0.45), "{tail:?}");
        // Lightning all along it: far more often than one big cell.
        let mut lt = Lightning::new();
        let mut n = 0;
        for i in 0..36000 {
            lt.step(1.0 / 60.0, i as f64 / 60.0, &wx, p, p, &|_| 0.0);
            n += lt.thunder.drain(..).count();
        }
        assert!(n > 150, "{n} thunderclaps in 10 minutes");
        // The ground under the line is in its shade.
        let under = (p - e * 30_000.0).normalize() * geo::R;
        assert!(shade(&wx, under) > 0.5);
    }

    #[test]
    fn a_supercell_is_the_worst_of_them() {
        let mut wx = Weather::new(0.3);
        wx.storms = 0.0;
        (wx.wind, wx.wind_from) = (10.0, 270.0);
        let p = geo::lla_to_pos(0.72, -1.68, 400.0);
        wx.place_storm(p, 0.0, 0.0, crate::weather::SUPER_R, crate::weather::CellKind::parse("supercell"));
        wx.cells = wx.storm_cells(p);
        let (e, n, _) = geo::enu(p);
        let at = |east: f64, north: f64, alt: f64| local(&wx, (p + e * east + n * north).normalize() * (geo::R + alt), alt, 0.0);
        // The rotating updraft: 30 m/s and more in its core, extreme turbulence.
        let core = (0..12).map(|k| at(-2500.0 + 200.0 * k as f64, 0.0, wx.base() as f64 + 5000.0)).fold(Local::default(), |a, l| if l.draft > a.draft { l } else { a });
        assert!(core.draft > 30.0 && core.sigma > 6.0, "{core:?}");
        // Its core, downwind and to the left (north of a westerly): very heavy rain and hail.
        let r = crate::weather::SUPER_R * 1000.0;
        let (sa, sx, _) = shaft_at(1.0);
        let wet = at(sa * r, sx * r, 600.0);
        assert!(wet.rain > 0.9 && wet.hail > 0.5 && wet.draft < -5.0, "{wet:?}");
        // The rain-free base over the updraft, upwind.
        let dry = at(-0.4 * r, -0.2 * r, 600.0);
        assert!(dry.rain < 0.1 && dry.hail == 0.0, "{dry:?}");
        // The anvil shades the ground 60 km downwind, and still a little at 100 km.
        assert!(shade(&wx, (p + e * 60_000.0).normalize() * geo::R) > 0.3);
        assert!(shade(&wx, (p + e * 100_000.0).normalize() * geo::R) > 0.01);
        // Near-continuous lightning: several times a big ordinary storm's.
        let mut lt = Lightning::new();
        let mut nf = 0;
        let mut far = 0;
        for i in 0..36000 {
            lt.step(1.0 / 60.0, i as f64 / 60.0, &wx, p, p, &|_| 0.0);
            for f in &lt.flashes {
                if f.age == -f.leader {
                    nf += 1;
                    let foot = f.segs.iter().filter(|s| s.level == 0).map(|s| s.b).fold(f.segs[0].b, |a, b| if b.length() < a.length() { b } else { a });
                    if f.kind == Kind::Ground && (foot - p).dot(e) > 2.0 * r {
                        far += 1;
                    }
                }
            }
        }
        assert!(nf > 300 && far > 2, "{nf} flashes, {far} strikes far out under the anvil in 10 minutes");
    }

    /// The worst turbulence (sigma) and drafts met crossing a weather `setup`
    /// placed at `p` along a few lines through it, from 400 m up to well into
    /// the tower, as the flight model gets it (weather.rs `air`).
    fn roughest(setup: impl Fn(&mut Weather, DVec3)) -> (f64, f64, f64) {
        let mut wx = Weather::new(0.3);
        wx.storms = 0.0;
        (wx.wind, wx.wind_from) = (8.0, 270.0);
        let p = geo::lla_to_pos(0.72, -1.68, 0.0);
        setup(&mut wx, p);
        wx.cells = wx.storm_cells(p);
        let (e, n, _) = geo::enu(p);
        let (mut sig, mut up, mut dn) = (0.0f64, 0.0f64, 0.0f64);
        for alt in [400.0, 1200.0, 2500.0, 4000.0, 6000.0] {
            for k in -100..=100 {
                for off in [e * 1.0, n * 1.0, e - n * 0.3, n + e * 0.4] {
                    let x = off.normalize() * (k as f64 * 250.0);
                    let q = (p + x).normalize() * (geo::R + alt);
                    // (at a few moments, as it comes in bursts)
                    for t in [0.0, 4.0, 9.0, 15.0] {
                        let a = wx.air(q, alt, t, false);
                        sig = sig.max(a.sigma_w);
                        up = up.max(a.draft);
                        dn = dn.min(a.draft);
                    }
                }
            }
        }
        (sig, up, dn)
    }

    #[test]
    fn turbulence_ranks_the_kinds_of_weather() {
        use crate::weather::CellKind;
        let deck = roughest(|wx, p| {
            wx.rain_set = Some(1.0);
            for _ in 0..600 {
                wx.step_deck(1.0, p, Some(0.0));
            }
        });
        let shower = roughest(|wx, p| wx.place_storm(p, 0.0, 0.0, 1.8, Some(CellKind::Shower)));
        let thunder = roughest(|wx, p| wx.place_storm(p, 0.0, 0.0, 5.0, Some(CellKind::Thunder)));
        let severe = roughest(|wx, p| wx.place_storm(p, 0.0, 0.0, 6.0, None));
        let squall = roughest(|wx, p| wx.place_squall(p, 0.0, 0.0, 60.0));
        let sup = roughest(|wx, p| wx.place_storm(p, 0.0, 0.0, crate::weather::SUPER_R, Some(CellKind::Supercell)));
        for (k, v) in [("deck", deck), ("shower", shower), ("thunder", thunder), ("severe", severe), ("squall", squall), ("supercell", sup)] {
            println!("{k:10} sigma {:4.1}  draft {:+5.1} {:+5.1}", v.0, v.1, v.2);
        }
        // The rain deck: smooth to a light chop, no storm drafts.
        assert!(deck.0 > 0.5 && deck.0 < 1.5 && deck.1 < 1.0 && deck.2 > -1.0, "{deck:?}");
        // A shower: light to moderate, a gentle updraft and a little sink.
        assert!(shower.0 > 1.5 && shower.0 < 3.0 && shower.1 > 2.0 && shower.1 < 6.0, "{shower:?}");
        // A thunderstorm: moderate to severe, a clear up- and downdraft couplet.
        assert!(thunder.0 > 4.0 && thunder.0 < 7.0 && thunder.1 > 4.0 && thunder.2 < -3.0, "{thunder:?}");
        // Severe storms and squall lines: severe.
        assert!(severe.0 > thunder.0 + 1.0 && severe.0 < 9.5 && severe.1 > thunder.1, "{severe:?}");
        assert!(squall.0 > thunder.0 + 1.0 && squall.0 < 9.5, "{squall:?}");
        // A supercell: extreme, an updraft of 30 m/s and more.
        assert!(sup.0 > severe.0.max(squall.0) + 2.0 && sup.1 > 30.0, "{sup:?}");
        assert!(deck.0 < shower.0 && shower.0 < thunder.0);
    }

    #[test]
    fn storm_turbulence_builds_gradually_and_ends_a_few_km_out() {
        let (wx, p) = storm();
        let (e, n, _) = geo::enu(p);
        let r = 6000.0;
        // Coming in from the side (no anvil overhead), 1500 m up.
        let at = |d: f64| local(&wx, (p + n * d).normalize() * (geo::R + 1500.0), 1500.0, 0.0).sigma;
        let s: Vec<f64> = [4.0, 3.0, 2.5, 2.0, 1.5, 1.2].iter().map(|k| at(k * r)).collect();
        println!("{s:?}");
        assert!(s[0] == 0.0 && s[1] < 0.3, "back to the day's own air a few km clear: {s:?}");
        assert!(s.windows(2).all(|w| w[1] >= w[0]) && s[3] > 0.3 && s[5] < 3.0, "building gradually: {s:?}");
        // Downwind, far out under the anvil: a light chop.
        let anvil = local(&wx, (p + e * 4.0 * r).normalize() * (geo::R + 1500.0), 1500.0, 0.0).sigma;
        assert!(anvil > 0.2 && anvil < 1.2, "{anvil}");
    }

    #[test]
    fn lightning_flashes_and_thunders() {
        let (wx, p) = storm();
        let mut lt = Lightning::new();
        lt.forced.push(Forced { at: 1.0, kind: Kind::Ground, km: 3.0, bearing: 90.0 });
        let mut t = 0.0;
        let mut peak = 0.0f32;
        let mut segs = Vec::new();
        while t < 1.2 {
            lt.step(1.0 / 60.0, t, &wx, p, p, &|_| 0.0);
            let mut g = [[0.0; 4]; 2 * MAX_FLASHES];
            if lt.globals(p, 1.0 / 60.0, &mut g) > 0 {
                peak = peak.max(g[0][3]);
            }
            if (t - 1.0).abs() < 0.01 {
                lt.segments(p, 1.0 / 60.0, &mut segs);
            }
            t += 1.0 / 60.0;
        }
        assert!(peak > 0.8, "{peak}");
        assert!(segs.len() > 100, "{}", segs.len());
        let th = lt.thunder[0];
        assert!((th.delay - th.near / 343.0).abs() < 0.1 && th.near > 2500.0 && th.far > th.near, "{th:?}");
        // A storm left alone flashes now and then on its own.
        let mut lt = Lightning::new();
        let mut n = 0;
        for i in 0..36000 {
            lt.step(1.0 / 60.0, i as f64 / 60.0, &wx, p, p, &|_| 0.0);
            n += lt.thunder.drain(..).count();
        }
        assert!((20..=200).contains(&n), "{n} thunderclaps in 10 minutes");
    }
}
