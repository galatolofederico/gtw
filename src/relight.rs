//! Taking the sunlight out of the 3D tiles' photographs (tiles3d.rs), so the
//! shader can light them with our own sun (photo.wgsl, `photo_shade`).
//!
//! Google's textures are aerial photographs: sunlit surfaces and the shadows
//! of the moment they were taken. Shadowed texels are several times darker
//! than lit ones and bluer (lit by the sky alone), which is how they are found
//! here, texel by texel: the atlases are cut into hundreds of small islands, so
//! anything that looks at neighbours mixes unrelated surfaces. Each shadowed
//! texel is lifted by the tile's own lit / shadow ratio (per channel, so the
//! blue cast goes too), and how much it was in the sun goes into the alpha
//! channel: the shader keeps the photographed shadows when our sun stands
//! where the photographer's did, and casts its own when it does not.
//!
//! The photographer's sun is found from the mesh: walls facing it are lit,
//! walls facing away are in shadow (`wall_sun`).

use glam::Vec3;

/// Off with FABLE_NO_RELIGHT set: the photographs as they come, lit flat (to compare).
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FABLE_NO_RELIGHT").is_none())
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn to_lin() -> &'static [f32; 256] {
    static T: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

/// Linear to sRGB bytes, 4096 steps over 0..1 (enough below the first byte).
fn to_srgb(v: f32) -> u8 {
    static T: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let t = T.get_or_init(|| {
        (0..4096)
            .map(|i| {
                let c = (i as f32 + 0.5) / 4096.0;
                let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    });
    t[((v * 4096.0) as usize).min(4095)]
}

const LUM: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// Below this share of the tile's bright texels a texel may be in shadow
/// (fully from `DARK_FULL` down), if it is bluer than the lit ones by `BLUE_*`.
const DARK_START: f32 = 0.42;
const DARK_FULL: f32 = 0.17;
const BLUE_START: f32 = -0.015;
const BLUE_FULL: f32 = 0.05;
/// lit / shadow brightness, measured per tile within these bounds
const K_MIN: f32 = 2.0;
const K_MAX: f32 = 6.0;
/// used where a tile has too few lit or shadowed texels to measure it
const K_DEFAULT: [f32; 3] = [5.0, 4.6, 3.8];

/// What `delight` found (read by the tests).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    /// share of the texels at least half in shadow
    pub shadow: f32,
    /// the lift given to fully shadowed texels, per channel
    pub k: [f32; 3],
}

/// Lifts the shadows out of an RGBA8 (sRGB) image in place; alpha becomes how
/// lit each texel was (255 in the sun, 0 in full shadow).
pub fn delight(px: &mut [u8]) -> Stats {
    let lin = to_lin();
    let n = px.len() / 4;
    // Unused atlas space is black: left out of everything.
    let valid = |p: &[u8]| p[0].max(p[1]).max(p[2]) > 4;
    let mut lum: Vec<f32> = Vec::with_capacity(n);
    for p in px.chunks_exact(4).filter(|p| valid(p)) {
        lum.push(lin[p[0] as usize] * LUM[0] + lin[p[1] as usize] * LUM[1] + lin[p[2] as usize] * LUM[2]);
    }
    if lum.len() < 100 {
        for p in px.chunks_exact_mut(4) {
            p[3] = 255;
        }
        return Stats { shadow: 0.0, k: [1.0; 3] };
    }
    // Most of a tile is in the sun: its bright texels are the reference.
    let k75 = lum.len() * 3 / 4;
    let l_ref = *lum.select_nth_unstable_by(k75, |a, b| a.total_cmp(b)).1;
    let l_ref = l_ref.max(1e-4);
    let blue = |c: [f32; 3]| c[2] / (c[0] + c[1] + c[2] + 1e-4);
    let colour = |p: &[u8]| [lin[p[0] as usize], lin[p[1] as usize], lin[p[2] as usize]];
    let luma = |c: [f32; 3]| c[0] * LUM[0] + c[1] * LUM[1] + c[2] * LUM[2];
    let mut lit_blue: Vec<f32> = px.chunks_exact(4).filter(|p| valid(p)).map(colour).filter(|&c| luma(c) > 0.5 * l_ref).map(blue).collect();
    let b_lit = if lit_blue.len() > 50 {
        let m = lit_blue.len() / 2;
        *lit_blue.select_nth_unstable_by(m, |a, b| a.total_cmp(b)).1
    } else {
        0.33
    };
    // How much in shadow each texel is (kept in alpha for now: 255 = shadow).
    let (mut sum_lit, mut sum_sh, mut n_lit, mut n_sh, mut n_half) = ([0.0f64; 3], [0.0f64; 3], 0usize, 0usize, 0usize);
    for p in px.chunks_exact_mut(4) {
        if !valid(p) {
            p[3] = 0;
            continue;
        }
        let c = colour(p);
        let l = luma(c);
        let s = smooth(DARK_START, DARK_FULL, l / l_ref) * smooth(BLUE_START, BLUE_FULL, blue(c) - b_lit);
        p[3] = (s * 255.0).round() as u8;
        if s > 0.5 {
            n_half += 1;
        }
        if s > 0.8 {
            n_sh += 1;
            for i in 0..3 {
                sum_sh[i] += c[i] as f64;
            }
        } else if s < 0.05 && l > 0.5 * l_ref {
            n_lit += 1;
            for i in 0..3 {
                sum_lit[i] += c[i] as f64;
            }
        }
    }
    // The lift: the lit / shadow brightness ratio, and the colour shift
    // between them (sky light is blue), both measured on this tile.
    let k = if n_sh > 200 && n_lit > 200 {
        let ml = sum_lit.map(|v| (v / n_lit as f64) as f32);
        let ms = sum_sh.map(|v| (v / n_sh as f64) as f32);
        let (ll, ls) = (luma(ml).max(1e-5), luma(ms).max(1e-5));
        let kl = (ll / ls).clamp(K_MIN, K_MAX);
        [0, 1, 2].map(|i| kl * ((ml[i] / ll) / (ms[i] / ls).max(1e-3)).clamp(0.6, 1.6))
    } else {
        K_DEFAULT
    };
    let lk = k.map(f32::ln);
    for p in px.chunks_exact_mut(4) {
        if !valid(p) {
            p[3] = 255;
            continue;
        }
        let s = p[3] as f32 / 255.0;
        if s > 0.0 {
            for i in 0..3 {
                p[i] = to_srgb(lin[p[i] as usize] * (lk[i] * s).exp());
            }
        }
        p[3] = 255 - p[3];
    }
    Stats { shadow: n_half as f32 / n as f32, k }
}

/// Where the photographer's sun stood, from the walls of a tile: the lit share
/// of each wall triangle (alpha of `px`, as `delight` left it) against the way
/// it faces. Returns the sum over walls of area x (lit - mean lit) x facing
/// (horizontal unit vector, our frame) and the sum of their areas; summed
/// over many tiles, the direction is the sun's azimuth and the length over
/// the area how clearly the walls show it.
pub fn wall_sun(pos: &[Vec3], idx: &[u32], uv: &[[f32; 2]], px: &[u8], w: u32, h: u32, up: Vec3) -> (Vec3, f32) {
    let lit_at = |u: [f32; 2]| {
        let x = ((u[0] * w as f32) as i64).clamp(0, w as i64 - 1) as usize;
        let y = ((u[1] * h as f32) as i64).clamp(0, h as i64 - 1) as usize;
        px[(y * w as usize + x) * 4 + 3] as f32 / 255.0
    };
    let mut walls: Vec<(Vec3, f32, f32)> = Vec::new();
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let f = (b - a).cross(c - a);
        let area = f.length();
        if area < 1e-4 {
            continue;
        }
        let nrm = f / area;
        if nrm.dot(up).abs() > 0.3 {
            continue;
        }
        let Some(hz) = (nrm - up * nrm.dot(up)).try_normalize() else { continue };
        let (ua, ub, uc) = (uv[t[0] as usize], uv[t[1] as usize], uv[t[2] as usize]);
        let mid = [(ua[0] + ub[0] + uc[0]) / 3.0, (ua[1] + ub[1] + uc[1]) / 3.0];
        walls.push((hz, area, lit_at(mid)));
    }
    let total: f32 = walls.iter().map(|w| w.1).sum();
    if walls.len() < 50 || total <= 0.0 {
        return (Vec3::ZERO, 0.0);
    }
    let mean = walls.iter().map(|w| w.1 * w.2).sum::<f32>() / total;
    (walls.iter().map(|w| w.0 * (w.1 * (w.2 - mean))).sum(), total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lit grey half and its shadowed, bluer half: the shadow is lifted to
    /// the lit colour and marked in alpha.
    #[test]
    fn lifts_shadow() {
        let lit = [180u8, 175, 170, 255];
        let sh = [68u8, 76, 94, 255];
        let mut px: Vec<u8> = (0..4096).flat_map(|i| if i % 3 == 0 { sh } else { lit }).collect();
        let st = delight(&mut px);
        assert!((st.shadow - 1.0 / 3.0).abs() < 0.01, "{st:?}");
        let (l, s) = (&px[4..8], &px[0..4]);
        assert_eq!(l, &[180, 175, 170, 255]);
        assert_eq!(s[3], 0);
        for i in 0..3 {
            assert!((s[i] as i32 - l[i] as i32).abs() <= 5, "{s:?} vs {l:?}");
        }
    }

    /// Walls facing east lit, walls facing west dark: the sun is east.
    #[test]
    fn sun_from_walls() {
        let (up, e) = (Vec3::Z, Vec3::X);
        let mut pos = Vec::new();
        let mut uv = Vec::new();
        let mut idx = Vec::new();
        for k in 0..60 {
            let x = k as f32 * 3.0;
            for (side, u) in [(1.0f32, 0.25f32), (-1.0, 0.75)] {
                let b = pos.len() as u32;
                // a wall square to x, facing +x (side 1) or -x
                pos.extend([Vec3::new(x, 0.0, 0.0), Vec3::new(x, side, 0.0), Vec3::new(x, 0.0, 1.0)]);
                uv.extend([[u, 0.5]; 3]);
                idx.extend([b, b + 1, b + 2]);
            }
        }
        // left half of the texture lit, right half dark
        let px: Vec<u8> = (0..16).flat_map(|i| [0, 0, 0, if i % 4 < 2 { 255 } else { 0 }]).collect();
        let (a, area) = wall_sun(&pos, &idx, &uv, &px, 4, 4, up);
        assert!(area > 0.0);
        assert!(a.normalize().dot(e) > 0.99, "{a}");
    }
}
