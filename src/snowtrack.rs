//! Dynamic tracks and crash trails carved through snow on the ground.
//!
//! A toroidal 2D texture (`t_snowtrack`, `N x N`, `Rg8Unorm`) tracks snow
//! disturbance around the aircraft on a local tangent plane:
//! - R (`clear`): how deeply snow has been compressed or scraped away (0 = untouched,
//!   1 = bare ground revealed beneath).
//! - G (`berm`): displaced snow piled up along the rims of a wheel rut or crash trench.
//!
//! The active window scrolls lazily only when a new mark comes within `MARGIN`
//! cells of an edge, preserving ~460 m of continuous trail behind a rolling
//! landing or skidding crash without shifting while the camera orbits it.
//! Only dirty sub-rectangles (typically a few KB per frame) are uploaded to
//! the GPU.

use crate::geo;
use glam::{DVec2, DVec3};

/// Texels across the snow-track texture (render.rs `snow_track_tex`, Rg8Unorm).
/// Must match `SNOW_TR_N` in common.wgsl.
pub const N: usize = 2048;
/// Side of one texel in metres (2048 * 0.25 = 512 m window).
/// Must match `SNOW_TR_CELL` in common.wgsl.
pub const CELL: f64 = 0.25;
/// Look-ahead margin (cells) kept ahead of new marks when scrolling (48 m),
/// leaving ~464 m of trail behind along the direction of travel.
const MARGIN: i32 = 192;
/// Re-anchor the local tangent plane if a mark lands farther than this from
/// the current anchor (m), keeping Earth curvature error under ~1 mm.
const REANCHOR_DIST: f64 = 20_000.0;

/// One capsule-shaped disturbance stamped into the snow (landing gear tyre,
/// scraping airframe, sliding wreckage or initial crash blast).
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    /// Start and end of the contact segment on the ground (planet frame, m).
    pub a: DVec3,
    pub b: DVec3,
    /// Half-width of the cleared trough (m). Displaced snow berms extend to ~2x this.
    pub radius: f32,
    /// Snow removal strength in the trough, 0..1 (wheel ~0.8, hull/crash ~0.95..1.0).
    pub clear: f32,
    /// Raised snow berm strength along the rims, 0..1.
    pub berm: f32,
    /// Edge roughness 0..1 (0 = neat tyre rut, 0.7..1.0 = jagged crash gouge).
    pub rough: f32,
}

/// A non-wrapping sub-rectangle of `t_snowtrack` to upload this frame (Rg8Unorm).
pub struct Upload {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Rect {
    fn area(self) -> usize {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }

    fn union(self, o: Self) -> Self {
        Self {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
}

#[derive(Default)]
pub struct SnowTrack {
    /// N * N * 2 bytes (R = clear, G = berm), allocated lazily on the first mark.
    px: Vec<u8>,
    /// Tangent-plane anchor on the sea-level sphere, and its unit East / North axes.
    anchor: Option<DVec3>,
    east: DVec3,
    north: DVec3,
    /// Integer cell coordinates of the SW corner of the active [ox..ox+N, oy..oy+N) window.
    ox: i32,
    oy: i32,
    /// Whether any marks have been stamped since the last clear.
    active: bool,
    /// Dirty texel rectangles (non-wrapping, in 0..N) pending upload.
    dirty: Vec<Rect>,
    /// Full texture upload pending (e.g. after a re-anchor / jump).
    full_upload: bool,
}

impl SnowTrack {
    /// Stamps a snow track or crash trail capsule into the map.
    pub fn stamp(&mut self, m: Mark) {
        if m.radius <= 0.0 || (m.clear <= 0.0 && m.berm <= 0.0) {
            return;
        }
        let mid = (m.a + m.b) * 0.5;
        let need_anchor = match self.anchor {
            None => true,
            Some(a) => a.distance(mid) > REANCHOR_DIST,
        };
        if need_anchor {
            self.reanchor(mid);
        }
        let anchor = self.anchor.unwrap();
        let to_local = |p: DVec3| {
            let d = p - anchor;
            DVec2::new(d.dot(self.east), d.dot(self.north))
        };
        let a = to_local(m.a);
        let b = to_local(m.b);
        let r = (m.radius as f64).clamp(0.15, 40.0);
        let reach = r * 2.1 + CELL;

        // Scroll the toroidal window so the mark stays inside with look-ahead room.
        let cx = ((a.x + b.x) * 0.5 / CELL).floor() as i32;
        let cy = ((a.y + b.y) * 0.5 / CELL).floor() as i32;
        self.ensure_window(cx, cy);

        let ix0 = (((a.x.min(b.x) - reach) / CELL).floor() as i32).max(self.ox);
        let ix1 = (((a.x.max(b.x) + reach) / CELL).floor() as i32).min(self.ox + N as i32 - 1);
        let iy0 = (((a.y.min(b.y) - reach) / CELL).floor() as i32).max(self.oy);
        let iy1 = (((a.y.max(b.y) + reach) / CELL).floor() as i32).min(self.oy + N as i32 - 1);
        if ix0 > ix1 || iy0 > iy1 {
            return;
        }

        let ab = b - a;
        let ab2 = ab.length_squared();
        let inv_r = 1.0 / r;
        let clear_s = m.clear.clamp(0.0, 1.0) as f64;
        let berm_s = m.berm.clamp(0.0, 1.0) as f64;
        let rough = m.rough.clamp(0.0, 1.0) as f64;
        let reach2 = reach * reach;
        let mut changed = false;

        for iy in iy0..=iy1 {
            let py = (iy as f64 + 0.5) * CELL;
            let ty = iy.rem_euclid(N as i32) as usize;
            let row = ty * N;
            for ix in ix0..=ix1 {
                let px = (ix as f64 + 0.5) * CELL;
                let ap = DVec2::new(px - a.x, py - a.y);
                let t = if ab2 > 1e-10 { (ap.dot(ab) / ab2).clamp(0.0, 1.0) } else { 0.0 };
                let dx = ap.x - ab.x * t;
                let dy = ap.y - ab.y * t;
                let d2 = dx * dx + dy * dy;
                if d2 >= reach2 {
                    continue;
                }
                let mut u = d2.sqrt() * inv_r;
                if rough > 0.0 {
                    // Deterministic per-cell width variation so crash gouges look churned.
                    let h = cell_hash(ix, iy);
                    u *= 1.0 + rough * (h - 0.5) * 0.36;
                }
                let trough = 1.0 - smoothstep(0.55, 1.25, u);
                let new_clear = trough * clear_s;
                let berm_band = smoothstep(0.78, 1.32, u) * (1.0 - smoothstep(1.32, 2.05, u));
                let new_berm = berm_band * berm_s;
                if new_clear <= 1e-4 && new_berm <= 1e-4 {
                    continue;
                }

                let tx = ix.rem_euclid(N as i32) as usize;
                let k = (row + tx) * 2;
                let cur_c = self.px[k] as f64 / 255.0;
                let cur_b = self.px[k + 1] as f64 / 255.0;

                let fc = cur_c.max(new_clear);
                // A new track clearing this texel flattens any earlier snow berm here.
                let flat_b = cur_b * (1.0 - new_clear);
                let fb = flat_b.max(new_berm * (1.0 - fc * 0.85)).clamp(0.0, 1.0);

                let nc = (fc * 255.0 + 0.5) as u8;
                let nb = (fb * 255.0 + 0.5) as u8;
                if nc != self.px[k] || nb != self.px[k + 1] {
                    self.px[k] = nc;
                    self.px[k + 1] = nb;
                    changed = true;
                }
            }
        }
        if changed {
            self.active = true;
            self.mark_world_dirty(ix0, iy0, ix1, iy1);
        }
    }

    /// Re-anchors the tangent plane at `p` (projected to sea level) and clears all tracks.
    fn reanchor(&mut self, p: DVec3) {
        let (lat, lon, _) = geo::pos_to_lla(p);
        let c = geo::lla_to_pos(lat, lon, 0.0);
        let (e, n, _) = geo::enu(c);
        if self.px.is_empty() {
            self.px = vec![0u8; N * N * 2];
        } else {
            self.px.fill(0);
        }
        self.anchor = Some(c);
        self.east = e;
        self.north = n;
        let half = (N / 2) as i32;
        self.ox = -half;
        self.oy = -half;
        self.active = false;
        self.dirty.clear();
        self.full_upload = true;
    }

    /// Shifts the toroidal window if `(cx, cy)` approaches within `MARGIN` of an edge,
    /// clearing only the newly exposed strips of texels.
    fn ensure_window(&mut self, cx: i32, cy: i32) {
        let n = N as i32;
        let mut new_ox = self.ox;
        let mut new_oy = self.oy;
        if cx < self.ox + MARGIN {
            new_ox = cx - MARGIN;
        } else if cx >= self.ox + n - MARGIN {
            new_ox = cx - (n - 1 - MARGIN);
        }
        if cy < self.oy + MARGIN {
            new_oy = cy - MARGIN;
        } else if cy >= self.oy + n - MARGIN {
            new_oy = cy - (n - 1 - MARGIN);
        }
        let dx = new_ox - self.ox;
        let dy = new_oy - self.oy;
        if dx == 0 && dy == 0 {
            return;
        }
        if dx.abs() >= n || dy.abs() >= n {
            self.px.fill(0);
            self.ox = cx - n / 2;
            self.oy = cy - n / 2;
            self.dirty.clear();
            self.full_upload = true;
            return;
        }
        // Clear newly uncovered columns along X.
        if dx > 0 {
            self.clear_cols(self.ox + n, self.ox + n + dx - 1);
        } else if dx < 0 {
            self.clear_cols(new_ox, self.ox - 1);
        }
        self.ox = new_ox;
        // Clear newly uncovered rows along Y.
        if dy > 0 {
            self.clear_rows(self.oy + n, self.oy + n + dy - 1);
        } else if dy < 0 {
            self.clear_rows(new_oy, self.oy - 1);
        }
        self.oy = new_oy;
    }

    fn clear_cols(&mut self, ix0: i32, ix1: i32) {
        for ix in ix0..=ix1 {
            let tx = ix.rem_euclid(N as i32) as usize;
            for ty in 0..N {
                let k = (ty * N + tx) * 2;
                self.px[k] = 0;
                self.px[k + 1] = 0;
            }
        }
        for (x0, x1) in wrap_spans(ix0, ix1) {
            self.push_dirty(Rect { x0, y0: 0, x1, y1: N });
        }
    }

    fn clear_rows(&mut self, iy0: i32, iy1: i32) {
        for iy in iy0..=iy1 {
            let ty = iy.rem_euclid(N as i32) as usize;
            let row = ty * N * 2;
            self.px[row..row + N * 2].fill(0);
        }
        for (y0, y1) in wrap_spans(iy0, iy1) {
            self.push_dirty(Rect { x0: 0, y0, x1: N, y1 });
        }
    }

    fn mark_world_dirty(&mut self, ix0: i32, iy0: i32, ix1: i32, iy1: i32) {
        if self.full_upload {
            return;
        }
        for (x0, x1) in wrap_spans(ix0, ix1) {
            for (y0, y1) in wrap_spans(iy0, iy1) {
                self.push_dirty(Rect { x0, y0, x1, y1 });
            }
        }
    }

    fn push_dirty(&mut self, mut r: Rect) {
        if self.full_upload || r.x0 >= r.x1 || r.y0 >= r.y1 {
            return;
        }
        // Coalesce with another dirty rect only when the union barely adds area
        // (so perpendicular row + column clear strips stay two tiny uploads).
        let mut i = 0;
        while i < self.dirty.len() {
            let u = self.dirty[i].union(r);
            if u.area() <= self.dirty[i].area() + r.area() + 4096 {
                r = u;
                self.dirty.swap_remove(i);
                i = 0;
            } else {
                i += 1;
            }
        }
        self.dirty.push(r);
        if self.dirty.len() > 24 {
            // Fall back to a single full upload if heavily fragmented.
            self.dirty.clear();
            self.full_upload = true;
        }
    }

    /// Returns the dirty texture uploads to send to the GPU this frame.
    pub fn take_uploads(&mut self) -> Vec<Upload> {
        if self.px.is_empty() {
            return Vec::new();
        }
        if self.full_upload {
            self.full_upload = false;
            self.dirty.clear();
            return vec![Upload { x: 0, y: 0, w: N as u32, h: N as u32, data: self.px.clone() }];
        }
        if self.dirty.is_empty() {
            return Vec::new();
        }
        let rects: Vec<Rect> = self.dirty.drain(..).collect();
        let mut out = Vec::with_capacity(rects.len());
        for r in rects {
            let w = r.x1 - r.x0;
            let h = r.y1 - r.y0;
            let mut data = vec![0u8; w * h * 2];
            for row in 0..h {
                let src = ((r.y0 + row) * N + r.x0) * 2;
                let dst = row * w * 2;
                data[dst..dst + w * 2].copy_from_slice(&self.px[src..src + w * 2]);
            }
            out.push(Upload { x: r.x0 as u32, y: r.y0 as u32, w: w as u32, h: h as u32, data });
        }
        out
    }

    /// Globals `snow_tr_c`, `snow_tr_e`, `snow_tr_n`:
    /// - `c`: SW corner of the active window relative to `cam`, w = side length in m (0 = none).
    /// - `e`: unit East axis, w = toroidal wrap offset along East (m).
    /// - `n`: unit North axis, w = toroidal wrap offset along North (m).
    pub fn globals(&self, cam: DVec3) -> [[f32; 4]; 3] {
        if !self.active {
            return [[0.0; 4]; 3];
        }
        let Some(anchor) = self.anchor else { return [[0.0; 4]; 3] };
        let side = (N as f64 * CELL) as f32;
        let sw = anchor + self.east * (self.ox as f64 * CELL) + self.north * (self.oy as f64 * CELL) - cam;
        let off_x = (self.ox.rem_euclid(N as i32) as f64 * CELL) as f32;
        let off_y = (self.oy.rem_euclid(N as i32) as f64 * CELL) as f32;
        let v = |a: DVec3, w: f32| [a.x as f32, a.y as f32, a.z as f32, w];
        [v(sw, side), v(self.east, off_x), v(self.north, off_y)]
    }
}

/// Splits an inclusive world-cell range `[i0..=i1]` into 1 or 2 half-open texel spans in `0..N`.
fn wrap_spans(i0: i32, i1: i32) -> impl Iterator<Item = (usize, usize)> {
    let n = N as i32;
    let mut out = [(0usize, 0usize); 2];
    let mut len = 0;
    if i0 <= i1 {
        if i1 - i0 + 1 >= n {
            out[0] = (0, N);
            len = 1;
        } else {
            let a = i0.rem_euclid(n) as usize;
            let b = i1.rem_euclid(n) as usize;
            if a <= b {
                out[0] = (a, b + 1);
                len = 1;
            } else {
                out[0] = (a, N);
                out[1] = (0, b + 1);
                len = 2;
            }
        }
    }
    out.into_iter().take(len)
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cell_hash(ix: i32, iy: i32) -> f64 {
    let mut h = (ix as u32).wrapping_mul(0x27d4_eb2d) ^ (iy as u32).wrapping_add(0x9e37_79b9).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h >> 8) as f64 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples `(clear, berm)` from a GPU-side mirror buffer using the exact
    /// same formula as `common.wgsl::snow_track_at`.
    fn sample_like_shader(gpu: &[u8], g: [[f32; 4]; 3], cam: DVec3, p: DVec3) -> (f32, f32) {
        let size = g[0][3];
        if size <= 0.0 {
            return (0.0, 0.0);
        }
        let q = (p - cam).as_vec3() - glam::Vec3::new(g[0][0], g[0][1], g[0][2]);
        let e = glam::Vec3::new(g[1][0], g[1][1], g[1][2]);
        let n = glam::Vec3::new(g[2][0], g[2][1], g[2][2]);
        let lx = q.dot(e);
        let ly = q.dot(n);
        if lx < 0.0 || lx >= size || ly < 0.0 || ly >= size {
            return (0.0, 0.0);
        }
        let u = ((lx + g[1][3]) / size).rem_euclid(1.0);
        let v = ((ly + g[2][3]) / size).rem_euclid(1.0);
        let tx = ((u * N as f32).floor() as usize).min(N - 1);
        let ty = ((v * N as f32).floor() as usize).min(N - 1);
        let k = (ty * N + tx) * 2;
        (gpu[k] as f32 / 255.0, gpu[k + 1] as f32 / 255.0)
    }

    fn apply_uploads(gpu: &mut [u8], uploads: Vec<Upload>) {
        for u in uploads {
            let (x, y, w, h) = (u.x as usize, u.y as usize, u.w as usize, u.h as usize);
            for row in 0..h {
                let dst = ((y + row) * N + x) * 2;
                let src = row * w * 2;
                gpu[dst..dst + w * 2].copy_from_slice(&u.data[src..src + w * 2]);
            }
        }
    }

    #[test]
    fn wheel_track_clears_center_and_raises_berms() {
        let base = geo::lla_to_pos(46.0f64.to_radians(), 7.0f64.to_radians(), 1500.0);
        let (e, n, _) = geo::enu(base);
        let mut st = SnowTrack::default();
        let mut gpu = vec![0u8; N * N * 2];

        // Stamp a 20 m north-south wheel track at x = 0.
        st.stamp(Mark { a: base, b: base + n * 20.0, radius: 0.30, clear: 0.85, berm: 0.70, rough: 0.0 });
        apply_uploads(&mut gpu, st.take_uploads());
        assert_eq!(gpu, st.px);

        let g = st.globals(base);
        // Center of track: strongly cleared, no berm.
        let (c0, b0) = sample_like_shader(&gpu, g, base, base + n * 10.0);
        assert!(c0 > 0.75, "center clear {c0}");
        assert!(b0 < 0.25, "center berm {b0}");
        // Rim at ~1.35 * radius (~0.40 m): raised snow berm, mostly untouched clear.
        let (cr, br) = sample_like_shader(&gpu, g, base, base + n * 10.0 + e * 0.40);
        assert!(br > 0.35, "rim berm {br}");
        assert!(cr < 0.25, "rim clear {cr}");
        // Outside track (1.2 m away — where a C172 nose/main gap sits): untouched.
        let (co, bo) = sample_like_shader(&gpu, g, base, base + n * 10.0 + e * 1.2);
        assert_eq!((co, bo), (0.0, 0.0));
    }

    #[test]
    fn crossing_track_cuts_through_earlier_berm() {
        let base = geo::lla_to_pos(46.0f64.to_radians(), 7.0f64.to_radians(), 1500.0);
        let (e, n, _) = geo::enu(base);
        let mut st = SnowTrack::default();
        let mut gpu = vec![0u8; N * N * 2];

        st.stamp(Mark { a: base - n * 5.0, b: base + n * 5.0, radius: 0.5, clear: 0.9, berm: 0.85, rough: 0.0 });
        apply_uploads(&mut gpu, st.take_uploads());
        let g = st.globals(base);
        let (_, b_before) = sample_like_shader(&gpu, g, base, base + e * 0.68);
        assert!(b_before > 0.5, "berm before crossing: {b_before}");

        // Cross east-west right over that berm.
        st.stamp(Mark { a: base - e * 5.0, b: base + e * 5.0, radius: 0.5, clear: 0.95, berm: 0.85, rough: 0.0 });
        apply_uploads(&mut gpu, st.take_uploads());
        let (c_after, b_after) = sample_like_shader(&gpu, g, base, base + e * 0.68);
        assert!(c_after > 0.85, "cleared at intersection: {c_after}");
        assert!(b_after < 0.15, "old berm flattened at intersection: {b_after}");
    }

    #[test]
    fn toroidal_scroll_preserves_long_trail_and_syncs_uploads() {
        let base = geo::lla_to_pos(46.0f64.to_radians(), 7.0f64.to_radians(), 1500.0);
        let (_, n, _) = geo::enu(base);
        let mut st = SnowTrack::default();
        let mut gpu = vec![0u8; N * N * 2];

        // Roll 700 m north in 2 m steps (well past the 512 m texture side, forcing toroidal wrap).
        for step in 0..350 {
            let p0 = base + n * (step as f64 * 2.0);
            let p1 = base + n * ((step + 1) as f64 * 2.0);
            st.stamp(Mark { a: p0, b: p1, radius: 0.6, clear: 0.9, berm: 0.7, rough: 0.2 });
            apply_uploads(&mut gpu, st.take_uploads());
        }
        // Incremental dirty-rect uploads must keep the GPU copy bit-for-bit identical to CPU!
        assert_eq!(gpu, st.px);

        let end = base + n * 700.0;
        let g = st.globals(end);
        // Recent trail 20 m behind and 380 m behind are both still intact!
        let (c_near, _) = sample_like_shader(&gpu, g, end, base + n * 680.0);
        let (c_far, _) = sample_like_shader(&gpu, g, end, base + n * 320.0);
        assert!(c_near > 0.8, "20 m behind: {c_near}");
        assert!(c_far > 0.8, "380 m behind: {c_far}");
        // Untouched ground 20 m ahead must be clean (no wraparound ghost from 500 m back!).
        let (c_ahead, b_ahead) = sample_like_shader(&gpu, g, end, base + n * 720.0);
        assert_eq!((c_ahead, b_ahead), (0.0, 0.0));
    }

    #[test]
    fn diagonal_scroll_keeps_uploads_compact() {
        let base = geo::lla_to_pos(46.0f64.to_radians(), 7.0f64.to_radians(), 1500.0);
        let (e, n, _) = geo::enu(base);
        let mut st = SnowTrack::default();
        let mut gpu = vec![0u8; N * N * 2];
        st.stamp(Mark { a: base, b: base, radius: 0.5, clear: 0.9, berm: 0.7, rough: 0.0 });
        apply_uploads(&mut gpu, st.take_uploads());

        // Move diagonally NE past the scroll margin, checking each step stays far smaller than a full upload.
        let mut max_bytes = 0usize;
        for step in 0..220 {
            let d0 = (e + n) * (step as f64 * 1.5);
            let d1 = (e + n) * ((step + 1) as f64 * 1.5);
            st.stamp(Mark { a: base + d0, b: base + d1, radius: 0.5, clear: 0.9, berm: 0.7, rough: 0.0 });
            let ups = st.take_uploads();
            max_bytes = max_bytes.max(ups.iter().map(|u| u.data.len()).sum());
            apply_uploads(&mut gpu, ups);
        }
        assert_eq!(gpu, st.px);
        assert!(max_bytes < 256 * 1024, "diagonal scroll uploaded too much: {max_bytes} bytes");
    }

    #[test]
    fn shaders_use_the_same_constants() {
        let wgsl = include_str!("shaders/common.wgsl");
        let get = |name: &str| -> f64 {
            let line = wgsl.lines().find(|l| l.starts_with(&format!("const {name} ="))).unwrap_or_else(|| panic!("{name} missing"));
            line.split('=').nth(1).unwrap().split(';').next().unwrap().trim().parse().unwrap()
        };
        assert_eq!(get("SNOW_TR_N"), N as f64);
        assert_eq!(get("SNOW_TR_CELL"), CELL);
    }
}
