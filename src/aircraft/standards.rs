//! A light check that every built-in aircraft meets the basics AGENTS.md asks
//! for: a detailed outside, a cockpit the pilot sits in with a panel in
//! front and a view out, and geometry that agrees with the `Spec` (span,
//! collision points, gear, eye). Deliberately loose: it catches an aircraft
//! that is missing, mis-scaled or obviously unfinished, not mild clipping
//! or small mismatches, which need eyes on the model.

use super::*;
use crate::airframe::Vis;
use glam::Mat4;

/// Everything solid (not glass, not a propeller disc or chute), as the
/// `(piece, lo, hi)` boxes `cockpit::occluded` takes.
fn solid(pieces: &[crate::airframe::Piece]) -> Vec<(usize, Vec3, Vec3)> {
    pieces.iter().enumerate().filter(|(_, p)| !p.glass && p.vis == Vis::Always && !p.mesh.v.is_empty()).map(|(i, p)| {
        let (lo, hi) = p.mesh.v.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos))));
        (i, lo, hi)
    }).collect()
}

/// A view direction from yaw (right of ahead) and pitch (up), degrees.
fn dir(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(y.sin() * p.cos(), p.sin(), -y.cos() * p.cos())
}

/// Has a cockpit: someone aboard works its controls (the Starship has none).
fn crewed(spec: &Spec, pieces: &[crate::airframe::Piece]) -> bool {
    !cockpit_controls(spec, pieces).is_empty()
}

#[test]
fn aircraft_meet_the_basic_standard() {
    let mut bad = Vec::new();
    let mut keys = std::collections::HashSet::new();
    for spec in BUILTIN {
        let n = spec.name;
        let mut fail = |m: String| bad.push(format!("{n}: {m}"));

        // ---- identity and menu picture
        if spec.name.is_empty() || spec.keys.is_empty() || spec.blurb.is_empty() {
            fail("needs a name, a command-line key and a blurb".into());
        }
        for k in spec.keys {
            if !keys.insert(*k) {
                fail(format!("key {k:?} used by another aircraft"));
            }
        }
        let png = spec.still;
        let (w, h) = if png.len() > 24 && png[..8] == *b"\x89PNG\r\n\x1a\n" {
            (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()))
        } else {
            (0, 0)
        };
        if (w, h) != (680, 392) {
            fail(format!("menu picture should be a 680x392 PNG, is {w}x{h}"));
        }

        // ---- outside: detailed, at the Spec's scale, collision points on it
        let model = (spec.build)();
        let tris: usize = model.pieces.iter().map(|p| p.mesh.i.len() / 3).sum();
        let mats: std::collections::HashSet<u32> = model.pieces.iter().flat_map(|p| p.mesh.v.iter().map(|v| v.mat[0].to_bits())).collect();
        let (c, half) = model.bounds();
        let (lo, hi) = (c - half, c + half);
        let length = 2.0 * half.z;
        println!("{n}: {} pieces, {tris} triangles, {} materials, bounds {lo:.2} .. {hi:.2}, span {:.2} (b {:.2}), eye {:.2}",
            model.pieces.len(), mats.len(), spec.span, spec.b, spec.eye);
        if tris < 20_000 {
            fail(format!("only {tris} triangles: the outside needs more detail"));
        }
        if mats.len() < 3 {
            fail(format!("only {} materials: paint, glass, metal and rubber at least", mats.len()));
        }
        let crew = crewed(spec, &model.pieces);
        if crew {
            // the wings reach the tip station the flight model uses
            if (half.x - spec.span).abs() > 0.15 * spec.span.max(1.0) {
                fail(format!("model is {:.2} m wide each side but the wing tip station is {:.2} m", half.x, spec.span));
            }
            if (spec.b as f32 - 2.0 * spec.span).abs() > 0.1 * spec.b as f32 {
                fail(format!("span b {:.2} m disagrees with the tip station {:.2} m", spec.b, spec.span));
            }
        }
        // nose, tips, tail, fin top, belly, prop/pods: on or near the model
        let slack = Vec3::splat(0.3) + half * 0.08;
        for (i, p) in spec.hard.iter().enumerate() {
            let p = p.as_vec3();
            if p.cmplt(lo - slack).any() || p.cmpgt(hi + slack).any() {
                fail(format!("hard point {i} {p:.2} is off the model"));
            }
        }
        let nose = spec.hard[0].as_vec3();
        if (nose.z - lo.z).abs() > 0.1 * length + 0.3 {
            fail(format!("nose hard point z {:.2} is far from the model's nose {:.2}", nose.z, lo.z));
        }
        // gear legs under the airframe, tyres reaching about its lowest point
        for leg in spec.gear {
            let a = leg.at.as_vec3();
            if a.cmplt(lo - slack).any() || a.cmpgt(hi + slack).any() {
                fail(format!("gear leg {} at {a:.2} is off the model", leg.name));
            }
        }
        if let Some(low) = spec.gear.iter().map(|l| l.at.y as f32).reduce(f32::min) {
            if (low - lo.y).abs() > 0.1 * (2.0 * half.y) + 0.2 {
                fail(format!("lowest tyre {low:.2} m but the model's lowest point is {:.2} m", lo.y));
            }
        }

        // ---- cockpit: the pilot sits inside, a panel ahead, a view out
        if !crew {
            continue;
        }
        let eye = spec.eye;
        // (a fighter's eye sits up in the canopy, above the fuselage section)
        let (top, _, bottom) = (spec.fuse)(eye.z);
        if eye.cmplt(lo).any() || eye.cmpgt(hi).any() || eye.y > top + 1.0 || eye.y < bottom {
            fail(format!("eye {eye:.2} is not inside the fuselage (section {bottom:.2} .. {top:.2})"));
        }
        let rest = vec![Mat4::IDENTITY; model.pieces.len()];
        let solids = solid(&model.pieces);
        let hits = |d: Vec3, far: f32| crate::cockpit::occluded(&model.pieces, &rest, &solids, eye, d, far);
        // looking down at the instruments, something solid within reach
        let panel = [-10.0, 0.0, 10.0].iter().flat_map(|&y| [-15.0, -25.0, -35.0].map(|p| (y, p))).filter(|&(y, p)| hits(dir(y, p), 1.6)).count();
        // looking ahead and a little up, out through the windscreen: clear of
        // geometry, or through a window the shader cuts in a skin (which the
        // aircraft's own `sees_inside` knows about)
        let clear = |d: Vec3| !hits(d, 50.0) || !(spec.sees_inside)(eye, d);
        let out = [-20.0, -10.0, 0.0, 10.0, 20.0].iter().flat_map(|&y| [0.0, 5.0, 10.0].map(|p| (y, p))).filter(|&(y, p)| clear(dir(y, p))).count();
        println!("    panel rays hit {panel}/9, clear view-out rays {out}/15");
        if panel < 3 {
            fail(format!("nothing solid ahead of and below the eye, where the panel goes ({panel}/9 rays hit within 1.6 m)"));
        }
        if out < 3 {
            fail(format!("no clear view out ahead ({out}/15 rays clear to 50 m)"));
        }
        if !model.pieces.iter().any(|p| p.glass) {
            fail("no glass: a canopy or windscreen".into());
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}
