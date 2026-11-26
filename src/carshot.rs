//! A small software rasteriser for looking at the procedural vehicle meshes
#![allow(non_snake_case)]
//! without starting the game: `GTW_CARSHOT=out.png cargo test --bin gtw
//! carshot -- --ignored` writes a contact sheet of every model from four
//! angles, shaded roughly as `inst.wgsl` shades vehicles.
use crate::render::MeshVertex;
use glam::{Mat4, Vec3, Vec4Swizzles};

const SS: usize = 3;

struct Target {
    col: Vec<Vec3>,
    z: Vec<f32>,
}

fn sky(d: Vec3) -> Vec3 {
    let t = d.y.clamp(-1.0, 1.0);
    if t > 0.0 {
        Vec3::new(0.55, 0.68, 0.85).lerp(Vec3::new(0.25, 0.4, 0.7), t)
    } else {
        Vec3::new(0.32, 0.3, 0.27)
    }
}

fn shade(v: &MeshVertex, n: Vec3, p: Vec3, eye: Vec3, tint: Vec3) -> Vec3 {
    let m = v.col[3];
    let paint = (m < 0.1) as u8 as f32;
    let glass = (m > 0.4 && m < 0.6) as u8 as f32;
    let head = (m > 0.15 && m < 0.35) as u8 as f32;
    let tail = (m > 0.65 && m < 0.85) as u8 as f32;
    let chrome = (m > 0.86 && m < 0.95) as u8 as f32;
    let col = Vec3::from_slice(&v.col[..3]);
    let albedo = col.lerp(tint, paint);
    let vd = (p - eye).normalize();
    let mut n = n.normalize_or_zero();
    if n.dot(vd) > 0.0 {
        n = -n;
    }
    let sun = Vec3::new(0.45, 0.8, -0.4).normalize();
    let lam = n.dot(sun).max(0.0);
    let amb = 0.35 + 0.15 * n.y;
    let mut c = albedo * (1.0 - glass * 0.5) * (lam * 1.0 + amb);
    let r = vd - 2.0 * vd.dot(n) * n;
    let ndv = (-vd).dot(n).max(0.0);
    let f0 = paint * 0.05 + glass * 0.08 + head * 0.08 + chrome * 0.6;
    let fres = f0 + (1.0 - f0) * (1.0 - ndv).powi(5) * (paint * 0.6 + glass + head * 0.5 + chrome);
    let hh = (sun - vd).normalize();
    let spec = n.dot(hh).max(0.0).powf(220.0) * (paint + glass + chrome) * 2.5;
    c += sky(r) * fres + Vec3::splat(spec * 0.3);
    c += head * Vec3::new(1.0, 0.95, 0.8) * 0.15 + tail * Vec3::new(1.0, 0.04, 0.02) * 0.15;
    c
}

fn draw(
    W: usize,
    H: usize,
    cols: usize,
    t: &mut Target,
    mesh: &(Vec<MeshVertex>, Vec<u32>),
    model: Mat4,
    vp: Mat4,
    eye: Vec3,
    tint: Vec3,
    ox: usize,
    oy: usize,
) {
    let (w, h) = (W * SS, H * SS);
    let stride = w * cols;
    let (vs, is) = mesh;
    let nm = model.inverse().transpose();
    let wv: Vec<(Vec3, Vec3, glam::Vec4)> = vs
        .iter()
        .map(|v| {
            let p = model.transform_point3(Vec3::from(v.pos));
            let n = nm.transform_vector3(Vec3::from(v.nrm));
            (p, n, vp * p.extend(1.0))
        })
        .collect();
    for tri in is.chunks(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let cl = [wv[a].2, wv[b].2, wv[c].2];
        if cl.iter().any(|q| q.w <= 0.05) {
            continue;
        }
        let sp: Vec<Vec3> = cl
            .iter()
            .map(|q| {
                let n = q.xyz() / q.w;
                Vec3::new(
                    (n.x * 0.5 + 0.5) * w as f32,
                    (0.5 - n.y * 0.5) * h as f32,
                    n.z,
                )
            })
            .collect();
        let area =
            (sp[1].x - sp[0].x) * (sp[2].y - sp[0].y) - (sp[2].x - sp[0].x) * (sp[1].y - sp[0].y);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = sp
            .iter()
            .map(|p| p.x)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as usize;
        let x1 = (sp.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil() as usize).min(w - 1);
        let y0 = sp
            .iter()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as usize;
        let y1 = (sp.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil() as usize).min(h - 1);
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let e = |i: usize, j: usize| {
                    (sp[j].x - sp[i].x) * (py - sp[i].y) - (px - sp[i].x) * (sp[j].y - sp[i].y)
                };
                let (l0, l1, l2) = (e(1, 2) / area, e(2, 0) / area, e(0, 1) / area);
                if l0 < 0.0 || l1 < 0.0 || l2 < 0.0 {
                    continue;
                }
                // Perspective-correct weights.
                let (q0, q1, q2) = (l0 / cl[0].w, l1 / cl[1].w, l2 / cl[2].w);
                let s = q0 + q1 + q2;
                let (q0, q1, q2) = (q0 / s, q1 / s, q2 / s);
                let z = l0 * sp[0].z + l1 * sp[1].z + l2 * sp[2].z;
                let k = (oy * h + y) * stride + ox * w + x;
                if z >= t.z[k] {
                    continue;
                }
                t.z[k] = z;
                let p = wv[a].0 * q0 + wv[b].0 * q1 + wv[c].0 * q2;
                let n = wv[a].1 * q0 + wv[b].1 * q1 + wv[c].1 * q2;
                t.col[k] = shade(&vs[a], n, p, eye, tint);
            }
        }
    }
}

fn view_proj(eye: Vec3, target: Vec3, fovy: f32, aspect: f32) -> Mat4 {
    let f = (target - eye).normalize();
    let s = f.cross(Vec3::Y).normalize();
    let u = s.cross(f);
    let view = Mat4::from_cols(
        glam::Vec4::new(s.x, u.x, -f.x, 0.0),
        glam::Vec4::new(s.y, u.y, -f.y, 0.0),
        glam::Vec4::new(s.z, u.z, -f.z, 0.0),
        glam::Vec4::new(-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0),
    );
    let (n, fa) = (0.1f32, 200.0f32);
    let g = 1.0 / (fovy * 0.5).tan();
    let proj = Mat4::from_cols(
        glam::Vec4::new(g / aspect, 0.0, 0.0, 0.0),
        glam::Vec4::new(0.0, g, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, fa / (n - fa), -1.0),
        glam::Vec4::new(0.0, 0.0, n * fa / (n - fa), 0.0),
    );
    proj * view
}

/// Body + wheels as the game places them: (body mesh, wheel mesh, wheel positions).
pub type Model = (
    (Vec<MeshVertex>, Vec<u32>),
    Option<((Vec<MeshVertex>, Vec<u32>), [[f32; 3]; 4])>,
    f32,
);

pub fn sheet(models: &[Model], path: &str, W: usize, H: usize) {
    if models.len() == 1 {
        return grid(&models[0], path, W, H);
    }
    render(models, path, W, H, &VIEWS);
}

const VIEWS: [(f32, f32); 4] = [(-0.75, 0.22), (2.45, 0.25), (-1.5708, 0.05), (0.0, 0.08)];

fn render(models: &[Model], path: &str, W: usize, H: usize, views: &[(f32, f32)]) {
    let cols = views.len();
    let rows = models.len();
    let (w, h) = (W * SS, H * SS);
    let mut t = Target {
        col: vec![Vec3::new(0.8, 0.82, 0.84); w * h * cols * rows],
        z: vec![f32::MAX; w * h * cols * rows],
    };
    let tints = [
        Vec3::new(0.5, 0.04, 0.03),
        Vec3::new(0.05, 0.12, 0.35),
        Vec3::new(0.6, 0.6, 0.58),
        Vec3::new(0.08, 0.2, 0.08),
        Vec3::new(0.75, 0.75, 0.72),
        Vec3::new(0.7, 0.45, 0.02),
        Vec3::new(0.75, 0.75, 0.72),
        Vec3::new(0.4, 0.4, 0.4),
    ];
    for (r, (body, wheels, len)) in models.iter().enumerate() {
        for (c, &(yaw, pitch)) in views.iter().enumerate() {
            let dist = len.max(3.5) * 1.55;
            // yaw 0: looking at the nose (front is -z).
            let dir = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            let target = Vec3::new(0.0, 0.8, 0.0);
            let eye = target + dir * dist;
            let vp = view_proj(eye, target, 0.62, W as f32 / H as f32);
            let tint = tints[r % tints.len()];
            // Ground.
            let g = 30.0;
            let ground = (
                vec![
                    MeshVertex {
                        pos: [-g, 0.0, -g],
                        nrm: [0.0, 1.0, 0.0],
                        col: [0.42, 0.42, 0.42, 1.0],
                    },
                    MeshVertex {
                        pos: [g, 0.0, -g],
                        nrm: [0.0, 1.0, 0.0],
                        col: [0.42, 0.42, 0.42, 1.0],
                    },
                    MeshVertex {
                        pos: [g, 0.0, g],
                        nrm: [0.0, 1.0, 0.0],
                        col: [0.42, 0.42, 0.42, 1.0],
                    },
                    MeshVertex {
                        pos: [-g, 0.0, g],
                        nrm: [0.0, 1.0, 0.0],
                        col: [0.42, 0.42, 0.42, 1.0],
                    },
                ],
                vec![0, 1, 2, 0, 2, 3],
            );
            draw(
                W,
                H,
                cols,
                &mut t,
                &ground,
                Mat4::IDENTITY,
                vp,
                eye,
                tint,
                c,
                r,
            );
            draw(
                W,
                H,
                cols,
                &mut t,
                body,
                Mat4::IDENTITY,
                vp,
                eye,
                tint,
                c,
                r,
            );
            if let Some((wm, pos)) = wheels {
                for p in pos {
                    draw(
                        W,
                        H,
                        cols,
                        &mut t,
                        wm,
                        Mat4::from_translation(Vec3::from(*p)),
                        vp,
                        eye,
                        tint,
                        c,
                        r,
                    );
                }
            }
        }
    }
    let mut img = image::RgbImage::new((W * cols) as u32, (H * rows) as u32);
    let stride = w * cols;
    for y in 0..H * rows {
        for x in 0..W * cols {
            let mut acc = Vec3::ZERO;
            for sy in 0..SS {
                for sx in 0..SS {
                    acc += t.col[(y * SS + sy) * stride + x * SS + sx];
                }
            }
            let c = acc / (SS * SS) as f32;
            let c = c / (c + 0.6) * 1.6;
            let srgb = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
            img.put_pixel(
                x as u32,
                y as u32,
                image::Rgb([srgb(c.x), srgb(c.y), srgb(c.z)]),
            );
        }
    }
    img.save(path).unwrap();
}

/// One model, its four views in a 2 x 2 grid.
fn grid(m: &Model, path: &str, W: usize, H: usize) {
    let mut tiles = Vec::new();
    for k in 0..4 {
        let p = format!("{path}.{k}.png");
        render(std::slice::from_ref(m), &p, W, H, &VIEWS[k..k + 1]);
        tiles.push(image::open(&p).unwrap().to_rgb8());
        let _ = std::fs::remove_file(&p);
    }
    let mut img = image::RgbImage::new(2 * W as u32, 2 * H as u32);
    for (k, t) in tiles.iter().enumerate() {
        image::imageops::replace(&mut img, t, (k % 2 * W) as i64, (k / 2 * H) as i64);
    }
    img.save(path).unwrap();
}

#[test]
#[ignore]
fn carshot() {
    let path = std::env::var("GTW_CARSHOT").unwrap_or_else(|_| "carshot.png".into());
    let player: Vec<Model> = crate::cars::player_models()
        .into_iter()
        .map(|m| (m.body, Some((m.wheel, m.wheels)), m.len))
        .collect();
    let only: Option<usize> = std::env::var("GTW_CARSHOT_MODEL")
        .ok()
        .and_then(|s| s.parse().ok());
    let player: Vec<Model> = player
        .into_iter()
        .enumerate()
        .filter(|(i, _)| only.is_none_or(|o| o == *i))
        .map(|(_, m)| m)
        .collect();
    let big = only.is_some();
    sheet(
        &player,
        &path,
        if big { 760 } else { 360 },
        if big { 440 } else { 220 },
    );
    if only.is_none() {
        let traffic: Vec<Model> = crate::cars::build_models()
            .into_iter()
            .zip(crate::cars::CAR_LEN)
            .map(|(m, l)| (m, None, l))
            .collect();
        sheet(&traffic, &path.replace(".png", "_traffic.png"), 360, 220);
        let far: Vec<Model> = crate::cars::build_far_models()
            .into_iter()
            .zip(crate::cars::CAR_LEN)
            .map(|(m, l)| (m, None, l))
            .collect();
        sheet(&far, &path.replace(".png", "_far.png"), 360, 220);
        let near = crate::cars::build_models();
        for (i, ((nv, ni), (fv, fi))) in near
            .iter()
            .zip(crate::cars::build_far_models().iter())
            .enumerate()
        {
            println!(
                "model {i}: near {} tris {} verts, far {} tris {} verts",
                ni.len() / 3,
                nv.len(),
                fi.len() / 3,
                fv.len()
            );
        }
    }
}
