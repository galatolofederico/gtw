//! The VFR chart over the moving map: airports and radio beacons drawn the way
//! aeronautical charts draw them, the stations the radios are tuned to, and
//! the aircraft's track.
//!
//! The symbols follow the ICAO and FAA VFR charts:
//! - an airport with a hard runway is a filled circle crossed by its longest
//!   runway; a grass or dirt field is a ring. Blue for the airports with
//!   scheduled flights (the ones with a tower, as near as the data tells),
//!   magenta for the others;
//! - a VOR is a hexagon, inside a square when it has a DME, with a compass
//!   rose when the map is close enough;
//! - an NDB is a dotted circle.
//!
//! The world has no magnetic variation (see gauges.rs), so the roses and the
//! courses are true.

use crate::{airports, geo, nav};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use glam::DVec3;
use std::collections::VecDeque;

pub const TOWERED: Color32 = Color32::from_rgb(92, 168, 255);
pub const UNTOWERED: Color32 = Color32::from_rgb(232, 96, 214);
pub const VOR: Color32 = Color32::from_rgb(120, 205, 255);
pub const NDB: Color32 = Color32::from_rgb(222, 140, 206);
/// the stations NAV 1 and the ADF are tuned to.
pub const TUNED: Color32 = Color32::from_rgb(110, 240, 140);
pub const TRACK: Color32 = Color32::from_rgb(255, 208, 84);
const HALO: Color32 = Color32::from_black_alpha(200);

/// Which layers the chart shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layers {
    pub airports: bool,
    pub navaids: bool,
}

/// The stations the radios are tuned to, copied from `nav::Radios` each frame.
#[derive(Clone, Debug, Default)]
pub struct Tuned {
    pub nav: Option<nav::Station>,
    pub obs: f64,
    pub adf: Option<nav::Station>,
}

impl Tuned {
    fn is(&self, st: &nav::Station) -> bool {
        let same = |t: &Option<nav::Station>| t.as_ref().is_some_and(|t| t.kind == st.kind && t.ident == st.ident && (t.lat - st.lat).abs() < 1e-4 && (t.lon - st.lon).abs() < 1e-4);
        same(&self.nav) || same(&self.adf)
    }
}

/// Something on the chart the pointer can point at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// an index into `airports::Db::apts`
    Airport(u32),
    /// an index into `nav::db()`
    Navaid(u32),
}

impl Target {
    /// where it is, degrees.
    pub fn lat_lon(self) -> (f64, f64) {
        match self {
            Target::Airport(i) => {
                let a = &airports::Db::get().apts[i as usize];
                (a.lat, a.lon)
            }
            Target::Navaid(i) => {
                let s = &nav::db()[i as usize];
                (s.lat, s.lon)
            }
        }
    }

    /// "EBAW Antwerp International Airport", "BUB Brussels".
    pub fn name(self) -> String {
        match self {
            Target::Airport(i) => {
                let a = &airports::Db::get().apts[i as usize];
                format!("{} {}", a.ident, a.name)
            }
            Target::Navaid(i) => nav::db()[i as usize].label(),
        }
    }
}

/// A symbol drawn this frame, for the pointer.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub at: Pos2,
    pub target: Target,
}

/// The nearest symbol within `reach` pixels of `p`.
pub fn hit_at(hits: &[Hit], p: Pos2, reach: f32) -> Option<Target> {
    hits.iter().map(|h| (h.at.distance(p), h.target)).filter(|(d, _)| *d <= reach).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, t)| t)
}

/// What one paint of the chart covers.
pub struct Scene<'a> {
    pub rect: Rect,
    pub zoom: u8,
    /// the latitudes it covers and the longitudes either side of `lon` (degrees)
    pub lat0: f64,
    pub lat1: f64,
    pub lon: f64,
    pub half_lon: f64,
    /// which way north is on the screen (a unit vector)
    pub north: Vec2,
    /// a point (degrees) on the screen.
    pub project: &'a dyn Fn(f64, f64) -> Pos2,
    /// whether a label fits inside the map.
    pub fits: &'a dyn Fn(Rect) -> bool,
}

impl Scene<'_> {
    fn covers(&self, lat: f64, lon: f64) -> bool {
        let dlon = (lon - self.lon + 540.0).rem_euclid(360.0) - 180.0;
        lat >= self.lat0 && lat <= self.lat1 && (self.half_lon >= 180.0 || dlon.abs() <= self.half_lon)
    }
    /// a bearing (degrees true) as a screen direction.
    fn dir(&self, bearing: f64) -> Vec2 {
        rotate(self.north, bearing.to_radians())
    }
}

fn rotate(v: Vec2, angle: f64) -> Vec2 {
    let (s, c) = (angle as f32).sin_cos();
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// The smallest airport kind (0 small, 1 medium, 2 large) shown at a zoom, if any.
pub fn airport_kind_from(zoom: u8) -> Option<u8> {
    match zoom {
        0..=4 => None,
        5..=7 => Some(2),
        8 => Some(1),
        _ => Some(0),
    }
}

fn airport_named(kind: u8, zoom: u8) -> bool {
    zoom >= [10, 9, 6][kind.min(2) as usize]
}

/// From which zoom a kind of beacon shows, and from which it is named.
pub fn navaid_zooms(kind: nav::Kind) -> (u8, u8) {
    match kind {
        nav::Kind::Vor => (7, 8),
        _ => (8, 10),
    }
}

/// Above this zoom the map draws the runways themselves.
const RUNWAYS_FROM: u8 = 11;
/// From this zoom VORs get their compass rose.
const ROSE_FROM: u8 = 10;
/// No more airports than this in one paint.
const MAX_AIRPORTS: usize = 700;

struct Label {
    order: u8,
    at: Pos2,
    clear: f32,
    text: String,
    color: Color32,
}

/// Paints the chart: the tuned courses, the beacons, the airports and their
/// names, keeping names off `occupied` and off one another. Returns what it
/// drew, for the pointer.
pub fn draw(painter: &Painter, s: &Scene, layers: Layers, tuned: &Tuned, occupied: &mut Vec<Rect>) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut labels = Vec::new();
    let on_screen = |p: Pos2| s.rect.expand(24.0).contains(p);
    courses(painter, s, tuned);
    if layers.navaids {
        for (i, st) in nav::db().iter().enumerate() {
            let tuned_here = tuned.is(st);
            let (from, named) = navaid_zooms(st.kind);
            if (s.zoom < from && !tuned_here) || !s.covers(st.lat, st.lon) {
                continue;
            }
            let p = (s.project)(st.lat, st.lon);
            if !on_screen(p) {
                continue;
            }
            let color = if tuned_here { TUNED } else if st.kind == nav::Kind::Vor { VOR } else { NDB };
            let clear = match st.kind {
                nav::Kind::Vor => {
                    vor(painter, p, st.dme, color, (s.zoom >= ROSE_FROM).then_some(s.north));
                    if st.dme { 10.0 } else { 8.0 }
                }
                _ => {
                    ndb(painter, p, color);
                    11.0
                }
            };
            hits.push(Hit { at: p, target: Target::Navaid(i as u32) });
            if s.zoom >= named || tuned_here {
                let mut text = format!("{} {}", st.ident, st.freq_text());
                if s.zoom >= named + 2 {
                    text = format!("{}\n{text}", st.name);
                }
                labels.push(Label { order: if tuned_here { 0 } else if st.kind == nav::Kind::Vor { 2 } else { 4 }, at: p, clear, text, color });
            }
        }
    }
    if let (true, Some(min_kind)) = (layers.airports, airport_kind_from(s.zoom)) {
        let db = airports::Db::get();
        let mut shown: Vec<(u32, Pos2)> = db.in_box(s.lat0, s.lat1, s.lon, s.half_lon, min_kind).into_iter()
            .map(|i| (i, (s.project)(db.apts[i as usize].lat, db.apts[i as usize].lon)))
            .filter(|(_, p)| on_screen(*p))
            .collect();
        // the big ones first, if there are too many.
        let rank = |i: u32| {
            let a = &db.apts[i as usize];
            (std::cmp::Reverse(a.kind), std::cmp::Reverse(a.sched), std::cmp::Reverse((a.longest() * 0.1) as i32))
        };
        shown.sort_by_key(|(i, _)| rank(*i));
        shown.truncate(MAX_AIRPORTS);
        for &(i, p) in shown.iter().rev() {
            let a = &db.apts[i as usize];
            let r = [5.0, 6.0, 7.0][a.kind.min(2) as usize];
            let color = if a.sched || a.kind == 2 { TOWERED } else { UNTOWERED };
            if s.zoom < RUNWAYS_FROM {
                airport(painter, p, r, a.any_paved(), runway_dir(s, a), color);
            }
            hits.push(Hit { at: p, target: Target::Airport(i) });
            // From RUNWAYS_FROM the runways carry their own labels.
            if airport_named(a.kind, s.zoom) && s.zoom < RUNWAYS_FROM {
                labels.push(Label { order: [5, 3, 1][a.kind.min(2) as usize], at: p, clear: r + 2.0, text: a.ident.clone(), color });
            }
        }
    }
    labels.sort_by_key(|l| l.order);
    for l in labels {
        label(painter, s, &l, occupied);
    }
    hits
}

/// The course NAV 1 is set to through its VOR, and the localizer of a tuned ILS.
fn courses(painter: &Painter, s: &Scene, tuned: &Tuned) {
    let Some(st) = &tuned.nav else { return };
    let far = s.rect.width().hypot(s.rect.height());
    let line = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], Stroke::new(4.0, Color32::from_black_alpha(110)));
        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.8, TUNED), 10.0, 6.0));
    };
    match st.kind {
        nav::Kind::Vor => {
            let p = (s.project)(st.lat, st.lon);
            let d = s.dir(tuned.obs);
            line(p - d * far, p + d * far);
        }
        nav::Kind::Ils => {
            // from the threshold out along the approach, 10 NM.
            let thr = (s.project)(st.thr.0, st.thr.1);
            let out = (s.project)(st.thr.0 - (st.course.to_radians().cos() * 0.1667), st.thr.1 - (st.course.to_radians().sin() * 0.1667 / st.thr.0.to_radians().cos().max(0.05)));
            line(thr, out);
            let d = (out - thr).normalized();
            let side = Vec2::new(-d.y, d.x);
            // the localizer's beam, a few degrees wide.
            painter.add(egui::Shape::convex_polygon(vec![thr + side * 2.0, out + side * 10.0, out - side * 10.0, thr - side * 2.0], TUNED.gamma_multiply(0.18), Stroke::new(1.0, TUNED.gamma_multiply(0.7))));
        }
        nav::Kind::Ndb => {}
    }
}

/// The longest runway's direction on the screen.
fn runway_dir(s: &Scene, a: &airports::Apt) -> Option<Vec2> {
    let r = a.rwys.iter().max_by(|x, y| x.len.total_cmp(&y.len))?;
    if let [Some(p0), Some(p1)] = r.pos {
        let d = (s.project)(p1.0, p1.1) - (s.project)(p0.0, p0.1);
        if d.length() > 1e-3 {
            return Some(d.normalized());
        }
    }
    let h = airports::ident_heading(&r.ident[0]).or_else(|| airports::ident_heading(&r.ident[1]).map(|h| h + 180.0))?;
    Some(s.dir(h as f64))
}

fn airport(painter: &Painter, p: Pos2, r: f32, paved: bool, runway: Option<Vec2>, color: Color32) {
    if paved {
        painter.circle_filled(p, r + 1.5, HALO);
        painter.circle_filled(p, r, color);
        if let Some(d) = runway {
            painter.line_segment([p - d * (r - 1.2), p + d * (r - 1.2)], Stroke::new(2.4, Color32::WHITE));
        }
    } else {
        painter.circle_stroke(p, r, Stroke::new(4.0, HALO));
        painter.circle_stroke(p, r, Stroke::new(2.0, color));
    }
}

fn vor(painter: &Painter, p: Pos2, dme: bool, color: Color32, rose: Option<Vec2>) {
    if let Some(north) = rose {
        let r = 34.0;
        painter.circle_stroke(p, r, Stroke::new(3.0, Color32::from_black_alpha(90)));
        painter.circle_stroke(p, r, Stroke::new(1.0, color.gamma_multiply(0.8)));
        for k in 0..36 {
            let d = rotate(north, (k as f64 * 10.0).to_radians());
            let len = if k % 9 == 0 { 9.0 } else if k % 3 == 0 { 6.0 } else { 3.5 };
            painter.line_segment([p + d * (r - len), p + d * r], Stroke::new(1.0, color.gamma_multiply(0.9)));
        }
        let tip = p + north * (r + 8.0);
        let side = Vec2::new(-north.y, north.x);
        painter.add(egui::Shape::convex_polygon(vec![tip, p + north * r + side * 4.0, p + north * r - side * 4.0], color, Stroke::NONE));
    }
    let hex: Vec<Pos2> = (0..6).map(|k| {
        let a = (k as f32) * std::f32::consts::FRAC_PI_3;
        p + Vec2::new(a.cos(), a.sin()) * 6.5
    }).collect();
    painter.add(egui::Shape::closed_line(hex.clone(), Stroke::new(4.0, HALO)));
    painter.add(egui::Shape::closed_line(hex, Stroke::new(1.8, color)));
    if dme {
        let sq = Rect::from_center_size(p, Vec2::splat(18.0));
        painter.rect_stroke(sq, 0.0, Stroke::new(3.5, HALO), egui::StrokeKind::Middle);
        painter.rect_stroke(sq, 0.0, Stroke::new(1.5, color), egui::StrokeKind::Middle);
    }
    painter.circle_filled(p, 2.4, HALO);
    painter.circle_filled(p, 1.6, color);
}

fn ndb(painter: &Painter, p: Pos2, color: Color32) {
    for (r, count) in [(4.0f32, 8), (7.0, 12), (10.0, 16)] {
        for k in 0..count {
            let a = k as f32 / count as f32 * std::f32::consts::TAU;
            let q = p + Vec2::new(a.cos(), a.sin()) * r;
            painter.circle_filled(q, 1.6, Color32::from_black_alpha(150));
            painter.circle_filled(q, 0.95, color);
        }
    }
    painter.circle_filled(p, 2.8, HALO);
    painter.circle_filled(p, 2.0, color);
}

fn label(painter: &Painter, s: &Scene, l: &Label, occupied: &mut Vec<Rect>) {
    let galley = painter.layout(l.text.clone(), crate::menu::semibold(11.0), l.color, f32::INFINITY);
    let size = galley.size() + Vec2::new(8.0, 4.0);
    let c = l.clear + 3.0;
    let spots = [
        Rect::from_min_size(l.at + Vec2::new(c, -size.y * 0.5), size),
        Rect::from_center_size(l.at + Vec2::new(0.0, c + size.y * 0.5), size),
        Rect::from_min_size(l.at + Vec2::new(-c - size.x, -size.y * 0.5), size),
        Rect::from_center_size(l.at - Vec2::new(0.0, c + size.y * 0.5), size),
    ];
    let Some(spot) = spots.into_iter().find(|r| (s.fits)(*r) && !occupied.iter().any(|o| o.intersects(r.expand(2.0)))) else { return };
    painter.rect_filled(spot, crate::menu::R_WIDGET, Color32::from_black_alpha(205));
    painter.rect_stroke(spot, crate::menu::R_WIDGET, Stroke::new(1.0, l.color.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    painter.galley(spot.min + Vec2::new(4.0, 2.0), galley, l.color);
    occupied.push(spot);
}

/// Where the aircraft has been: a point every 150 m, the last 600 km.
#[derive(Default)]
pub struct Trail {
    points: VecDeque<(f64, f64)>,
    last: Option<DVec3>,
    prev: Option<DVec3>,
}

const TRAIL_STEP: f64 = 150.0;
const TRAIL_POINTS: usize = 4000;
/// A jump this long between two frames is a respawn or a teleport, not flight.
const TRAIL_JUMP: f64 = 3000.0;

impl Trail {
    /// The aircraft is here now (degrees).
    pub fn push(&mut self, lat: f64, lon: f64) {
        let p = geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
        if self.prev.is_some_and(|q| (p - q).length() > TRAIL_JUMP) {
            self.clear();
        }
        self.prev = Some(p);
        if self.last.is_some_and(|q| (p - q).length() < TRAIL_STEP) {
            return;
        }
        self.last = Some(p);
        self.points.push_back((lat, lon));
        while self.points.len() > TRAIL_POINTS {
            self.points.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.last = None;
        self.prev = None;
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Paints the track up to the aircraft at `now`.
    pub fn draw(&self, painter: &Painter, s: &Scene, now: Pos2) {
        if self.points.is_empty() {
            return;
        }
        // Split where the line would cross the whole map (round the antimeridian).
        let jump = s.rect.width().max(s.rect.height()) * 0.5;
        let mut runs: Vec<Vec<Pos2>> = vec![Vec::new()];
        for p in self.points.iter().map(|&(la, lo)| (s.project)(la, lo)).chain(std::iter::once(now)) {
            let run = runs.last_mut().unwrap();
            if run.last().is_some_and(|q: &Pos2| q.distance(p) > jump) {
                runs.push(vec![p]);
            } else {
                run.push(p);
            }
        }
        for run in runs.into_iter().filter(|r| r.len() >= 2) {
            painter.add(egui::Shape::line(run.clone(), Stroke::new(4.5, Color32::from_black_alpha(120))));
            painter.add(egui::Shape::line(run, Stroke::new(2.0, TRACK)));
        }
    }
}

fn nm_text(m: f64) -> String {
    let nm = m / 1852.0;
    let text = if nm < 10.0 { format!("{nm:.1}") } else { format!("{nm:.0}") };
    text.replace('.', crate::i18n::lang().text("number.decimal_point"))
}

/// The card about what the pointer is on: name, runways or frequency, and how
/// far and which way from the aircraft (`from`, degrees).
pub fn card(painter: &Painter, near: Pos2, target: Target, from: (f64, f64), bounds: Rect) {
    let (lat, lon) = target.lat_lon();
    let here = geo::lla_to_pos(from.0.to_radians(), from.1.to_radians(), 0.0);
    let there = geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
    let range = crate::t!("vfr.range", nm = nm_text((there - here).length()), brg = format!("{:03.0}", nav::bearing(here, there)));
    let (title, color, mut lines) = match target {
        Target::Airport(i) => {
            let a = &airports::Db::get().apts[i as usize];
            let mut lines = vec![format!("{} · {}", if a.city.is_empty() { &a.country } else { &a.city }, crate::t!("vfr.elevation", ft = format!("{:.0}", a.elev / airports::FT)))];
            let mut rwys: Vec<_> = a.rwys.iter().collect();
            rwys.sort_by(|x, y| y.len.total_cmp(&x.len));
            for r in rwys.iter().take(4) {
                let surface = if r.paved { crate::t!("vfr.paved") } else { crate::t!("vfr.unpaved") };
                let mut line = format!("{}/{}  {:.0} m  {surface}", r.ident[0], r.ident[1], r.len);
                if r.lighted {
                    line += &format!(" · {}", crate::t!("vfr.lit"));
                }
                lines.push(line);
            }
            if rwys.len() > 4 {
                lines.push(crate::t!("vfr.more_runways", n = rwys.len() - 4));
            }
            (format!("{}  {}", a.ident, a.name), if a.sched || a.kind == 2 { TOWERED } else { UNTOWERED }, lines)
        }
        Target::Navaid(i) => {
            let st = &nav::db()[i as usize];
            let (kind, unit) = match (st.kind, st.dme) {
                (nav::Kind::Vor, true) => ("VOR-DME", "MHz"),
                (nav::Kind::Vor, false) => ("VOR", "MHz"),
                (_, true) => ("NDB-DME", "kHz"),
                _ => ("NDB", "kHz"),
            };
            (format!("{}  {}", st.ident, st.name), if st.kind == nav::Kind::Vor { VOR } else { NDB }, vec![format!("{kind}  {} {unit}", st.freq_text())])
        }
    };
    lines.push(range);
    lines.push(crate::t!("vfr.click_for_destination").to_string());
    let head = painter.layout_no_wrap(title, crate::menu::semibold(14.0), color);
    let body = painter.layout(lines.join("\n"), crate::menu::caption(), Color32::from_white_alpha(225), f32::INFINITY);
    let size = Vec2::new(head.size().x.max(body.size().x), head.size().y + 4.0 + body.size().y) + Vec2::splat(20.0);
    let mut at = near + Vec2::new(18.0, 14.0);
    if at.x + size.x > bounds.right() - 8.0 {
        at.x = near.x - 18.0 - size.x;
    }
    if at.y + size.y > bounds.bottom() - 8.0 {
        at.y = near.y - 14.0 - size.y;
    }
    let r = Rect::from_min_size(at, size);
    painter.rect_filled(r, crate::menu::R_PANEL, Color32::from_rgba_unmultiplied(14, 18, 24, 238));
    painter.rect_stroke(r, crate::menu::R_PANEL, Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    let head_h = head.size().y;
    painter.galley(r.min + Vec2::splat(10.0), head, color);
    painter.galley(r.min + Vec2::new(10.0, 14.0 + head_h), body, Color32::WHITE);
}

/// Icons for the map's layer buttons.
pub fn airport_icon(painter: &Painter, c: Pos2, color: Color32) {
    painter.circle_filled(c, 6.5, color);
    painter.line_segment([c + Vec2::new(-4.5, 3.0), c + Vec2::new(4.5, -3.0)], Stroke::new(2.0, Color32::from_black_alpha(220)));
}

pub fn navaid_icon(painter: &Painter, c: Pos2, color: Color32) {
    let hex: Vec<Pos2> = (0..6).map(|k| {
        let a = (k as f32) * std::f32::consts::FRAC_PI_3;
        c + Vec2::new(a.cos(), a.sin()) * 6.5
    }).collect();
    painter.add(egui::Shape::closed_line(hex, Stroke::new(1.8, color)));
    painter.circle_filled(c, 1.6, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_thresholds_grow_with_the_map() {
        assert_eq!(airport_kind_from(4), None);
        assert_eq!(airport_kind_from(5), Some(2));
        assert_eq!(airport_kind_from(7), Some(2));
        assert_eq!(airport_kind_from(8), Some(1));
        assert_eq!(airport_kind_from(12), Some(0));
        assert!(navaid_zooms(nav::Kind::Vor).0 < navaid_zooms(nav::Kind::Ndb).0);
    }

    #[test]
    fn the_box_finds_antwerp_and_wraps_the_antimeridian() {
        let db = airports::Db::get();
        let ebaw = db.find("EBAW").unwrap();
        assert!(db.in_box(51.0, 51.5, 4.4, 0.3, 0).contains(&ebaw));
        assert!(!db.in_box(51.0, 51.5, 4.4, 0.3, 2).contains(&ebaw), "EBAW is a medium airport");
        // Fiji's Nadi (NFFN, 177.4°E) from just across the antimeridian.
        let nffn = db.find("NFFN").unwrap();
        assert!(db.in_box(-18.5, -17.0, -179.0, 4.0, 0).contains(&nffn));
    }

    #[test]
    fn the_trail_steps_and_forgets_a_teleport() {
        let mut t = Trail::default();
        for k in 0..100 {
            t.push(50.0 + k as f64 * 0.0001, 4.0); // 11 m a frame
        }
        assert!((6..=8).contains(&t.len()), "{} points", t.len());
        t.push(52.0, 4.0);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn the_pointer_takes_the_nearest_symbol() {
        let hits = [Hit { at: Pos2::new(0.0, 0.0), target: Target::Airport(1) }, Hit { at: Pos2::new(10.0, 0.0), target: Target::Navaid(2) }];
        assert_eq!(hit_at(&hits, Pos2::new(7.0, 0.0), 12.0), Some(Target::Navaid(2)));
        assert_eq!(hit_at(&hits, Pos2::new(40.0, 0.0), 12.0), None);
    }

    #[test]
    fn north_up_bearings_point_the_right_way() {
        let east = rotate(Vec2::new(0.0, -1.0), 90f64.to_radians());
        assert!((east - Vec2::new(1.0, 0.0)).length() < 1e-5);
    }
}
