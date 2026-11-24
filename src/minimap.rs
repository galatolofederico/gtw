//! Satellite/road moving map. Bounded background tile loading; no second scene render.
mod data;

use crate::{geo::{self, TileId}, sources::Sources};
use egui::{Color32, Pos2, Stroke, Vec2};
use std::{collections::HashMap, sync::{Arc, mpsc}, time::{Duration, Instant}};

struct Tile {
    texture: Option<egui::TextureHandle>,
    touched: u64,
    retry: Instant,
}

struct MapTile {
    texture: Option<egui::TextureHandle>,
    places: Vec<data::Place>,
    touched: u64,
    retry: Instant,
}

pub const DEST_COLOR: Color32 = Color32::from_rgb(255, 110, 90);

#[derive(Clone, Debug)]
pub struct Destination {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

pub struct Minimap {
    pub visible: bool,
    pub landing_guidance: bool,
    pub heading_up: bool,
    pub zoom: u8,
    pub plan: bool,
    pub size: Vec2,
    /// Distance from the window's bottom edge, kept clear of the info panel below.
    pub bottom: f32,
    /// Lowest point of the HUD panel above, which the map must not reach.
    pub top: f32,
    resize_start: f32,
    /// The full-size map is open (J, or the button on the minimap's corner).
    pub full: bool,
    /// The full-size map's zoom, apart from the minimap's.
    pub full_zoom: u8,
    /// Where the full-size map is centred (Web Mercator, 0..1); None follows the aircraft.
    full_center: Option<glam::DVec2>,
    full_scroll: f32,
    pub destination: Option<Destination>,
    maps: HashMap<TileId, MapTile>,
    map_pending: Vec<(TileId, mpsc::Receiver<Option<data::Data>>)>,
    heading: f64,
    scroll: f32,
    tiles: HashMap<TileId, Tile>,
    pending: Vec<(TileId, u32, mpsc::Receiver<Option<image::RgbaImage>>)>,
    generation: u32,
    frame: u64,
    runway_airports: Vec<u32>,
    runway_center: Option<glam::DVec3>,
    runway_reach: f64,
    /// the VFR chart's layers, the radios' stations, the track flown
    pub layers: crate::vfrmap::Layers,
    pub tuned: crate::vfrmap::Tuned,
    pub trail: crate::vfrmap::Trail,
    /// the chart's symbols as last painted, for the pointer
    hits: Vec<crate::vfrmap::Hit>,
}

/// What one paint of the map shows: where, at what zoom, centred on what.
struct View {
    rect: egui::Rect,
    zoom: u8,
    /// tiles across the world at `zoom`
    n: f64,
    /// the centre, in tiles at `zoom`
    cx: f64,
    cy: f64,
    angle: f64,
    /// the centre's latitude and longitude (radians)
    lat: f64,
    lon: f64,
    /// how many place names it may show
    names: usize,
    /// loads it may have going at once
    loads: usize,
}

impl View {
    /// tiles to look at either side of the centre, across and down.
    fn reach(&self, factor: f64) -> (i64, i64) {
        let px = 256.0 * factor;
        if self.angle == 0.0 {
            (((self.rect.width() * 0.5) as f64 / px).ceil() as i64 + 1, ((self.rect.height() * 0.5) as f64 / px).ceil() as i64 + 1)
        } else {
            let r = (self.rect.width().hypot(self.rect.height()) * 0.5) as f64 / px;
            (r.ceil() as i64 + 1, r.ceil() as i64 + 1)
        }
    }
    /// a point (Web Mercator, 0..1) on the screen.
    fn project(&self, x: f64, y: f64) -> Pos2 {
        self.rect.center() + rotate(Vec2::new((tile_offset(x * self.n, self.cx, self.n) * 256.0) as f32, ((y * self.n - self.cy) * 256.0) as f32), self.angle)
    }
    /// The VFR chart's view of this one.
    fn scene<'a>(&self, project: &'a dyn Fn(f64, f64) -> Pos2, fits: &'a dyn Fn(egui::Rect) -> bool) -> crate::vfrmap::Scene<'a> {
        // a circle round the corners, so the heading-up map is covered too.
        let half = (self.rect.width().hypot(self.rect.height()) * 0.5) as f64 / 256.0;
        crate::vfrmap::Scene {
            rect: self.rect,
            zoom: self.zoom,
            lat0: geo::merc_y_to_lat(((self.cy + half) / self.n).min(1.0)).to_degrees(),
            lat1: geo::merc_y_to_lat(((self.cy - half) / self.n).max(0.0)).to_degrees(),
            lon: self.lon.to_degrees(),
            half_lon: half / self.n * 360.0,
            north: rotate(Vec2::new(0.0, -1.0), self.angle),
            project,
            fits,
        }
    }
    /// a screen point back to (lat, lon) in radians.
    fn unproject(&self, p: Pos2) -> (f64, f64) {
        let v = rotate(p - self.rect.center(), -self.angle) / 256.0;
        let mx = ((self.cx + v.x as f64) / self.n).rem_euclid(1.0);
        let my = ((self.cy + v.y as f64) / self.n).clamp(geo::lat_to_merc_y(85f64.to_radians()), geo::lat_to_merc_y(-85f64.to_radians()));
        (geo::merc_y_to_lat(my), mx * 2.0 * std::f64::consts::PI - std::f64::consts::PI)
    }
}

fn sub_corners(c: [Pos2; 4], u0: f32, v0: f32, u1: f32, v1: f32) -> [Pos2; 4] {
    let at = |u: f32, v: f32| {
        let top = c[0].to_vec2() * (1.0 - u) + c[1].to_vec2() * u;
        let bot = c[3].to_vec2() * (1.0 - u) + c[2].to_vec2() * u;
        (top * (1.0 - v) + bot * v).to_pos2()
    };
    [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)]
}

fn draw_quad(painter: &egui::Painter, tex_id: egui::TextureId, corners: [Pos2; 4], uv: [[f32; 2]; 4]) {
    let mut mesh = egui::Mesh::with_texture(tex_id);
    for (pos, uv) in corners.into_iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: Pos2::new(uv[0], uv[1]), color: Color32::WHITE });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(mesh);
}

fn nearest_airport_name(lat: f64, lon: f64, view: &View, click: Pos2) -> Option<String> {
    let db = crate::airports::Db::get();
    db.near(lat.to_degrees(), lon.to_degrees(), 25_000.0)
        .into_iter()
        .filter_map(|(id, _)| {
            let apt = &db.apts[id as usize];
            let pos = view.project(geo::lon_to_merc_x(apt.lon.to_radians()).rem_euclid(1.0), geo::lat_to_merc_y(apt.lat.to_radians()));
            let d = pos.distance(click);
            (d <= 16.0).then_some((d, apt.ident.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, ident)| ident)
}

fn rotate(v: Vec2, angle: f64) -> Vec2 {
    let (s, c) = (angle as f32).sin_cos();
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// the map's rounded square as half planes, for clipping.
struct Outline {
    rect: egui::Rect,
    edges: Vec<(Vec2, f32)>,
}

impl Outline {
    fn new(rect: egui::Rect) -> Self {
        let pts = crate::flightui::rounded(rect, crate::flightui::R_HUD.into());
        let c = rect.center();
        let edges = (0..pts.len())
            .filter_map(|i| {
                let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                let d = q - p;
                if d.length() < 1e-3 {
                    return None;
                }
                let mut n = Vec2::new(d.y, -d.x).normalized();
                if (c - p).dot(n) > 0.0 {
                    n = -n;
                }
                Some((n, p.to_vec2().dot(n)))
            })
            .collect();
        Self { rect, edges }
    }
    fn inside(&self, p: Pos2) -> bool {
        self.edges.iter().all(|(n, o)| p.to_vec2().dot(*n) <= *o)
    }
}

/// whether `r` is inside the map, `margin` in from its edge.
fn in_disc(r: egui::Rect, outline: &Outline, margin: f32) -> bool {
    let r = r.expand(margin);
    [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()].iter().all(|p| outline.inside(*p))
}

/// Spots labels must leave free: the aircraft, the north mark and the scale bar.
fn reserved(rect: egui::Rect, angle: f64) -> Vec<egui::Rect> {
    let (center, radius) = (rect.center(), rect.width() * 0.5);
    let north = center + rotate(Vec2::new(0.0, -(radius - 18.0)), angle);
    vec![
        egui::Rect::from_center_size(center, Vec2::splat(38.0)),
        egui::Rect::from_center_size(north, Vec2::splat(28.0)),
        egui::Rect::from_center_size(Pos2::new(center.x, rect.bottom() - 22.0), Vec2::new(84.0, 32.0)),
        // the resize grip and the full-size map button.
        egui::Rect::from_center_size(rect.right_top() + Vec2::new(-12.0, 12.0), Vec2::splat(24.0)),
        egui::Rect::from_center_size(rect.left_top() + Vec2::new(16.0, 16.0), Vec2::splat(30.0)),
    ]
}

fn lerp_vertex(a: &egui::epaint::Vertex, b: &egui::epaint::Vertex, t: f32) -> egui::epaint::Vertex {
    let c = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::epaint::Vertex {
        pos: a.pos + (b.pos - a.pos) * t,
        uv: a.uv + (b.uv - a.uv) * t,
        color: Color32::from_rgba_premultiplied(c(a.color.r(), b.color.r()), c(a.color.g(), b.color.g()), c(a.color.b(), b.color.b()), c(a.color.a(), b.color.a())),
    }
}

/// cuts a mesh to the map's outline.
fn clip_mesh(mesh: &egui::Mesh, outline: &Outline) -> egui::Mesh {
    let mut out = egui::Mesh::with_texture(mesh.texture_id);
    for tri in mesh.indices.chunks_exact(3) {
        let v = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize]);
        if v.iter().all(|v| outline.inside(v.pos)) {
            let base = out.vertices.len() as u32;
            out.vertices.extend(v);
            out.indices.extend([base, base + 1, base + 2]);
            continue;
        }
        let bounds = egui::Rect::from_points(&v.map(|v| v.pos));
        if !bounds.intersects(outline.rect) { continue; }
        let mut poly = v.to_vec();
        for (normal, offset) in &outline.edges {
            let side = |p: Pos2| offset - p.to_vec2().dot(*normal);
            let mut next = Vec::with_capacity(poly.len() + 1);
            for i in 0..poly.len() {
                let (p, q) = (&poly[i], &poly[(i + 1) % poly.len()]);
                let (sp, sq) = (side(p.pos), side(q.pos));
                if sp >= 0.0 { next.push(*p); }
                if (sp >= 0.0) != (sq >= 0.0) { next.push(lerp_vertex(p, q, sp / (sp - sq))); }
            }
            poly = next;
            if poly.len() < 3 { break; }
        }
        if poly.len() < 3 { continue; }
        let base = out.vertices.len() as u32;
        for i in 1..poly.len() as u32 - 1 { out.indices.extend([base, base + i, base + i + 1]); }
        out.vertices.extend(poly);
    }
    out
}

fn clip_shape(tessellator: &mut egui::epaint::Tessellator, shape: egui::Shape, outline: &Outline, out: &mut Vec<egui::Shape>) {
    let mesh = match shape {
        egui::Shape::Noop => return,
        egui::Shape::Vec(shapes) => {
            for s in shapes { clip_shape(tessellator, s, outline, out); }
            return;
        }
        egui::Shape::Mesh(mesh) => clip_mesh(&mesh, outline),
        other => {
            let mut mesh = egui::Mesh::default();
            tessellator.tessellate_shape(other, &mut mesh);
            clip_mesh(&mesh, outline)
        }
    };
    if !mesh.is_empty() { out.push(egui::Shape::mesh(mesh)); }
}

/// cuts the shapes painted since `start` to the map's outline.
fn clip_to_disc(ctx: &egui::Context, layer: egui::LayerId, start: egui::layers::ShapeIdx, outline: &Outline) {
    let font_size = ctx.fonts(|f| f.font_image_size());
    let mut tessellator = egui::epaint::Tessellator::new(ctx.pixels_per_point(), ctx.tessellation_options(|o| *o), font_size, Vec::new());
    ctx.graphics_mut(|g| {
        let list = g.entry(layer);
        let end = list.next_idx().0;
        for i in start.0..end {
            list.mutate_shape(egui::layers::ShapeIdx(i), |clipped| {
                let shape = std::mem::replace(&mut clipped.shape, egui::Shape::Noop);
                let mut meshes = Vec::new();
                clip_shape(&mut tessellator, shape, outline, &mut meshes);
                clipped.shape = egui::Shape::Vec(meshes);
            });
        }
    });
}

fn tile_offset(x: f64, center: f64, n: f64) -> f64 {
    (x - center + n * 0.5).rem_euclid(n) - n * 0.5
}

pub(crate) fn distance_label(meters: f64, lang: crate::i18n::Lang) -> String {
    let text = if meters >= 1000.0 { format!("{:.1} km", meters / 1000.0) } else { format!("{meters:.0} m") };
    text.replace('.', lang.text("number.decimal_point"))
}

impl Minimap {
    fn draw_runways(&mut self, world: &crate::world::World, painter: &egui::Painter, view: &View, outline: &Outline, occupied: &mut Vec<egui::Rect>) {
        // Below this zoom individual runways are too small to distinguish.
        if view.zoom < 9 { return; }
        let db = crate::airports::Db::get();
        // Far enough to cover the view (the full-size map reaches further), within reason.
        let meters_per_px = 2.0 * std::f64::consts::PI * geo::R * view.lat.cos() / (view.n * 256.0);
        let reach = ((view.rect.width().hypot(view.rect.height()) * 0.5) as f64 * meters_per_px * 1.2).clamp(120_000.0, 600_000.0);
        let position = geo::lla_to_pos(view.lat, view.lon, 0.0);
        if self.runway_center.is_none_or(|center| (position - center).length() > reach / 12.0) || (reach - self.runway_reach).abs() > self.runway_reach * 0.25 {
            self.runway_airports = db.near(view.lat.to_degrees(), view.lon.to_degrees(), reach).into_iter().map(|(id, _)| id).collect();
            self.runway_center = Some(position);
            self.runway_reach = reach;
        }
        let project = |(la, lo): (f64, f64)| view.project(geo::lon_to_merc_x(lo.to_radians()).rem_euclid(1.0), geo::lat_to_merc_y(la.to_radians()));
        let rect = view.rect;
        let color = Color32::from_rgb(95, 235, 255);
        for &id in &self.runway_airports {
            let airport = &db.apts[id as usize];
            // Prefer the exact geometry used by the 3D airfield and approach guide.
            // Some database entries (including EBNM) have no endpoint coordinates.
            let loaded = world.airfield(id).filter(|field| !field.geo.runways.is_empty());
            let runways: Vec<_> = if let Some(field) = loaded {
                field.geo.runways.iter().map(|r| {
                    let endpoints = [r.a, r.b].map(|p| {
                        let (lat, lon) = field.geo.lla(p);
                        (lat.to_degrees(), lon.to_degrees())
                    });
                    (endpoints, &r.ident)
                }).collect()
            } else {
                airport.rwys.iter().filter_map(|r| {
                    let [Some(a), Some(b)] = r.pos else { return None; };
                    Some(([a, b], &r.ident))
                }).collect()
            };
            for ([a, b], ident) in runways {
                let ends = [project(a), project(b)];
                if !egui::Rect::from_two_pos(ends[0], ends[1]).expand(4.0).intersects(rect) { continue; }
                let length = ends[0].distance(ends[1]);
                if length < 2.0 { continue; }
                painter.line_segment(ends, Stroke::new(6.0, Color32::from_black_alpha(220)));
                painter.line_segment(ends, Stroke::new(3.0, color));
                if view.zoom < 11 || length < 18.0 { continue; }
                let direction = (ends[1] - ends[0]) / length;
                let side = Vec2::new(-direction.y, direction.x);
                for end in ends {
                    painter.line_segment([end - side * 4.0, end + side * 4.0], Stroke::new(2.0, color));
                }
                let text = format!("{}  {}/{}", airport.ident, ident[0], ident[1]);
                let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), color);
                let midpoint = ends[0] + (ends[1] - ends[0]) * 0.5;
                let Some(label) = [-1.0, 1.0].into_iter().map(|sign| {
                    egui::Rect::from_center_size(midpoint + Vec2::new(0.0, sign * (galley.size().y * 0.5 + 8.0)), galley.size() + Vec2::new(6.0, 4.0))
                }).find(|r| in_disc(*r, outline, 6.0) && !occupied.iter().any(|other| other.intersects(*r))) else { continue; };
                painter.rect_filled(label, crate::menu::R_WIDGET, Color32::from_black_alpha(225));
                painter.galley(label.min + Vec2::new(3.0, 2.0), galley, color);
                occupied.push(label);
            }
        }
    }

    fn draw_plan_tile(&mut self, painter: &egui::Painter, id: TileId, corners: [Pos2; 4]) -> bool {
        let full_uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        if let Some(tile) = self.maps.get_mut(&id) {
            tile.touched = self.frame;
            if let Some(texture) = &tile.texture {
                draw_quad(painter, texture.id(), corners, full_uv);
                return true;
            }
        }
        let mut drawn = false;
        for az in (0..id.z).rev() {
            let (a, u, v, s) = id.ancestor(az);
            if let Some(tile) = self.maps.get_mut(&a) {
                tile.touched = self.frame;
                if let Some(texture) = &tile.texture {
                    let uv = [[u as f32, v as f32], [(u + s) as f32, v as f32], [(u + s) as f32, (v + s) as f32], [u as f32, (v + s) as f32]];
                    draw_quad(painter, texture.id(), corners, uv);
                    drawn = true;
                    break;
                }
            }
        }
        const QUADS: [(f32, f32, f32, f32); 4] = [(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 1.0, 0.5), (0.0, 0.5, 0.5, 1.0), (0.5, 0.5, 1.0, 1.0)];
        if id.z < 14 {
            for (c1, (u0, v0, u1, v1)) in id.children().into_iter().zip(QUADS) {
                let c1_corners = sub_corners(corners, u0, v0, u1, v1);
                if let Some(tile) = self.maps.get_mut(&c1) {
                    tile.touched = self.frame;
                    if let Some(texture) = &tile.texture {
                        draw_quad(painter, texture.id(), c1_corners, full_uv);
                        drawn = true;
                        continue;
                    }
                }
                if c1.z < 14 {
                    for (c2, (su0, sv0, su1, sv1)) in c1.children().into_iter().zip(QUADS) {
                        if let Some(tile) = self.maps.get_mut(&c2) {
                            tile.touched = self.frame;
                            if let Some(texture) = &tile.texture {
                                draw_quad(painter, texture.id(), sub_corners(c1_corners, su0, sv0, su1, sv1), full_uv);
                                drawn = true;
                            }
                        }
                    }
                }
            }
        }
        drawn
    }

    fn draw_imagery_tile(&mut self, painter: &egui::Painter, id: TileId, corners: [Pos2; 4]) -> bool {
        let full_uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        if let Some(tile) = self.tiles.get_mut(&id) {
            tile.touched = self.frame;
            if let Some(texture) = &tile.texture {
                draw_quad(painter, texture.id(), corners, full_uv);
                return true;
            }
        }
        let mut drawn = false;
        for az in (0..id.z).rev() {
            let (a, u, v, s) = id.ancestor(az);
            if let Some(tile) = self.tiles.get_mut(&a) {
                tile.touched = self.frame;
                if let Some(texture) = &tile.texture {
                    let uv = [[u as f32, v as f32], [(u + s) as f32, v as f32], [(u + s) as f32, (v + s) as f32], [u as f32, (v + s) as f32]];
                    draw_quad(painter, texture.id(), corners, uv);
                    drawn = true;
                    break;
                }
            }
        }
        const QUADS: [(f32, f32, f32, f32); 4] = [(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 1.0, 0.5), (0.0, 0.5, 0.5, 1.0), (0.5, 0.5, 1.0, 1.0)];
        if id.z < 17 {
            for (c1, (u0, v0, u1, v1)) in id.children().into_iter().zip(QUADS) {
                let c1_corners = sub_corners(corners, u0, v0, u1, v1);
                if let Some(tile) = self.tiles.get_mut(&c1) {
                    tile.touched = self.frame;
                    if let Some(texture) = &tile.texture {
                        draw_quad(painter, texture.id(), c1_corners, full_uv);
                        drawn = true;
                        continue;
                    }
                }
                if c1.z < 17 {
                    for (c2, (su0, sv0, su1, sv1)) in c1.children().into_iter().zip(QUADS) {
                        if let Some(tile) = self.tiles.get_mut(&c2) {
                            tile.touched = self.frame;
                            if let Some(texture) = &tile.texture {
                                draw_quad(painter, texture.id(), sub_corners(c1_corners, su0, sv0, su1, sv1), full_uv);
                                drawn = true;
                            }
                        }
                    }
                }
            }
        }
        drawn
    }

    /// The road map's tiles (when it is shown) and the place names over either map.
    fn draw_map(&mut self, src: &Arc<Sources>, painter: &egui::Painter, view: &View, outline: &Outline, occupied: &mut Vec<egui::Rect>) -> usize {
        let rect = view.rect;
        let z = view.zoom.min(14);
        let factor = 2f64.powi((view.zoom - z) as i32);
        let (mx, my, count) = (view.cx / factor, view.cy / factor, (1u32 << z) as i64);
        let (rx, ry) = view.reach(factor);
        let mut wanted = Vec::new();
        let mut visible = Vec::new();
        let mut drawn = 0;
        for dy in -ry..=ry { for dx in -rx..=rx {
            let (x, y) = (mx.floor() as i64 + dx, my.floor() as i64 + dy);
            if y < 0 || y >= count { continue; }
            let id = TileId::new(z, x.rem_euclid(count) as u32, y as u32);
            let offset = Vec2::new(tile_offset(x as f64, mx, count as f64) as f32, (y as f64 - my) as f32);
            let corners = [Vec2::ZERO, Vec2::X, Vec2::splat(1.0), Vec2::Y].map(|v| rect.center() + rotate((offset + v) * (256.0 * factor as f32), view.angle));
            if !egui::Rect::from_points(&corners).intersects(rect) { continue; }
            if !visible.contains(&id) { visible.push(id); }
            let d = dx * dx + dy * dy;
            if self.plan {
                wanted.push((0, d, id.ancestor(z.saturating_sub(2)).0));
                wanted.push((1, d, id));
                if self.draw_plan_tile(painter, id, corners) {
                    drawn += 1;
                }
            } else {
                wanted.push((1, d, id));
                if let Some(tile) = self.maps.get_mut(&id) {
                    tile.touched = self.frame;
                }
            }
        } }
        self.map_pending.retain(|(id, _)| id.z <= z);
        wanted.sort_by_key(|(priority, d, _)| (*priority, *d));
        let mut wanted = wanted.into_iter().map(|(_, _, id)| id)
            .filter(|id| self.maps.get(id).is_none_or(|t| t.texture.is_none() && Instant::now() >= t.retry));
        while self.map_pending.len() < view.loads {
            let Some(id) = wanted.find(|id| !self.map_pending.iter().any(|(p, _)| p == id)) else { break };
            let (tx, rx) = mpsc::channel();
            let src = Arc::clone(src);
            self.map_pending.push((id, rx));
            std::thread::spawn(move || { let _ = tx.send(data::load(&src, id)); });
        }
        let mut labels: Vec<_> = visible.iter().filter_map(|id| self.maps.get(id)).flat_map(|tile| &tile.places)
            .filter(|p| view.zoom >= p.min_zoom).collect();
        labels.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.name.cmp(&b.name)).then(a.xy[0].total_cmp(&b.xy[0])));
        let mut names = std::collections::HashSet::new();
        for place in labels {
            let name = place.label(crate::i18n::lang());
            if names.contains(name) { continue; }
            let point = view.project(place.xy[0], place.xy[1]);
            let font = egui::FontId::proportional(if place.priority == 0 { 13.0 } else { 11.0 });
            let galley = painter.layout_no_wrap(name.to_owned(), font, Color32::WHITE);
            // Try above and below so a town underneath the aircraft can still be named.
            let Some(label_rect) = [-1.0, 1.0].into_iter().map(|side| egui::Rect::from_center_size(point + Vec2::new(0.0, side * (galley.size().y * 0.5 + 6.0)), galley.size() + Vec2::new(6.0, 2.0)))
                .find(|r| in_disc(*r, outline, 6.0) && !occupied.iter().any(|other| other.intersects(r.expand(3.0)))) else { continue };
            painter.circle_filled(point, 2.5, Color32::WHITE);
            painter.circle_stroke(point, 2.5, Stroke::new(1.0, Color32::from_gray(40)));
            painter.rect_filled(label_rect, crate::menu::R_WIDGET, Color32::from_black_alpha(185));
            painter.galley(label_rect.min + Vec2::new(3.0, 1.0), galley, Color32::WHITE);
            occupied.push(label_rect);
            names.insert(name);
            if names.len() >= view.names { break; }
        }
        drawn
    }

    /// The satellite tiles under the view, the nearest coarse ones first, reusing
    /// a loaded ancestor where a tile is not in yet.
    fn draw_imagery(&mut self, src: &Arc<Sources>, painter: &egui::Painter, view: &View) -> usize {
        let (rect, zoom, n, cx, cy) = (view.rect, view.zoom, view.n, view.cx, view.cy);
        let (rx, ry) = view.reach(1.0);
        let mut wanted = Vec::new();
        let mut drawn = 0;
        for dy in -ry..=ry {
            for dx in -rx..=rx {
                let x = cx.floor() as i64 + dx;
                let y = cy.floor() as i64 + dy;
                if y < 0 || y >= n as i64 { continue; }
                let id = TileId::new(zoom, x.rem_euclid(n as i64) as u32, y as u32);
                let offset = Vec2::new(tile_offset(x as f64, cx, n) as f32, (y as f64 - cy) as f32) * 256.0;
                let corners = [Vec2::ZERO, Vec2::new(256.0, 0.0), Vec2::splat(256.0), Vec2::new(0.0, 256.0)]
                    .map(|v| rect.center() + rotate(offset + v, view.angle));
                if !egui::Rect::from_points(&corners).intersects(rect) { continue; }
                // Coarse coverage first, then detail; reuse ancestors and descendants while zooming.
                let coarse = id.ancestor(zoom.saturating_sub(2)).0;
                let d = dx * dx + dy * dy;
                wanted.push((0, d, coarse));
                wanted.push((1, d, id));
                if self.draw_imagery_tile(painter, id, corners) {
                    drawn += 1;
                }
            }
        }
        self.pending.retain(|(id, _, _)| id.z <= zoom);
        wanted.sort_by_key(|(priority, d, _)| (*priority, *d));
        let mut wanted = wanted.into_iter().map(|(_, _, id)| id)
            .filter(|id| self.tiles.get(id).is_none_or(|t| t.texture.is_none() && Instant::now() >= t.retry));
        while self.pending.len() < view.loads {
            let Some(id) = wanted.find(|id| !self.pending.iter().any(|(p, _, _)| p == id)) else { break };
            let (tx, rx) = mpsc::channel();
            let src = Arc::clone(src);
            self.pending.push((id, self.generation, rx));
            std::thread::spawn(move || {
                let img = src.imagery(id).ok().map(|img| image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Triangle));
                let _ = tx.send(img);
            });
        }
        drawn
    }

    /// Everything inside the map's outline: the imagery or road map, the
    /// runways (when flying), the approach guide, the names and the player/aircraft, cut to the outline.
    #[allow(clippy::too_many_arguments)]
    fn paint(&mut self, ctx: &egui::Context, src: &Arc<Sources>, painter: &egui::Painter, view: &View, aircraft: (f64, f64), guide: Option<&crate::approach::Guide>, world: &crate::world::World, mut occupied: Vec<egui::Rect>, aviation: bool, show_airports: bool) {
        let rect = view.rect;
        let outline = Outline::new(rect);
        // Everything painted from here to the bezel is cut to the outline afterwards.
        let first_shape = ctx.graphics_mut(|g| g.entry(painter.layer_id()).next_idx());
        painter.rect_filled(rect, crate::flightui::R_HUD, Color32::from_rgb(22, 32, 42));
        let supported = view.lat.abs() <= 85.0_f64.to_radians();
        let mut drawn = 0;
        if supported && !self.plan {
            drawn = self.draw_imagery(src, painter, view);
        }
        if supported {
            let map_drawn = self.draw_map(src, painter, view, &outline, &mut occupied);
            if self.plan { drawn = map_drawn; }
            if aviation {
                self.draw_runways(world, painter, view, &outline, &mut occupied);
                if let Some(guide) = guide { guide.draw_map(painter, rect, view.cx, view.cy, view.n, view.angle); }
            }
        }
        // the VFR chart (aviation) or airport pins (full ground map) and the track.
        let at = view.project(geo::lon_to_merc_x(aircraft.1).rem_euclid(1.0), geo::lat_to_merc_y(aircraft.0));
        self.hits.clear();
        if supported {
            let project = |la: f64, lo: f64| view.project(geo::lon_to_merc_x(lo.to_radians()).rem_euclid(1.0), geo::lat_to_merc_y(la.to_radians()));
            let fits = |r: egui::Rect| in_disc(r, &outline, 4.0);
            let scene = view.scene(&project, &fits);
            self.trail.draw(painter, &scene, at);
            if aviation || show_airports {
                let layers = if aviation {
                    self.layers
                } else {
                    crate::vfrmap::Layers { airports: self.layers.airports, navaids: false }
                };
                let empty_tuned = crate::vfrmap::Tuned::default();
                let tuned = if aviation { &self.tuned } else { &empty_tuned };
                self.hits = crate::vfrmap::draw(painter, &scene, layers, tuned, &mut occupied);
            }
        }
        if !supported || drawn == 0 {
            painter.text(rect.center() + Vec2::new(0.0, 32.0), egui::Align2::CENTER_CENTER,
                if !supported { crate::t!("minimap.map_unavailable_near_poles") }
                else if if self.plan { !self.map_pending.is_empty() } else { !self.pending.is_empty() } { crate::t!("minimap.loading_map") }
                else { crate::t!("minimap.imagery_unavailable") }, egui::FontId::proportional(12.0), Color32::WHITE);
        }
        // the destination route & pin, if set.
        if let Some(dest) = &self.destination {
            let dp = view.project(geo::lon_to_merc_x(dest.lon).rem_euclid(1.0), geo::lat_to_merc_y(dest.lat));
            painter.line_segment([at, dp], Stroke::new(4.0, Color32::from_black_alpha(130)));
            painter.line_segment([at, dp], Stroke::new(1.8, DEST_COLOR.gamma_multiply(0.85)));
            if outline.inside(dp) {
                let r = 6.5;
                let diamond = vec![dp + Vec2::new(0.0, -r), dp + Vec2::new(r, 0.0), dp + Vec2::new(0.0, r), dp + Vec2::new(-r, 0.0)];
                painter.add(egui::Shape::convex_polygon(diamond.clone(), DEST_COLOR, Stroke::new(1.5, Color32::from_black_alpha(200))));
                painter.circle_filled(dp, 2.0, Color32::WHITE);
                if let Some(name) = &dest.name {
                    let galley = painter.layout_no_wrap(name.clone(), crate::menu::semibold(11.0), Color32::WHITE);
                    let lr = egui::Rect::from_center_size(dp - Vec2::new(0.0, galley.size().y * 0.5 + 11.0), galley.size() + Vec2::new(8.0, 4.0));
                    if in_disc(lr, &outline, 4.0) {
                        painter.rect_filled(lr, crate::menu::R_WIDGET, Color32::from_black_alpha(200));
                        painter.galley(lr.min + Vec2::new(4.0, 2.0), galley, Color32::WHITE);
                    }
                }
            } else {
                // Off-edge indicator pointing toward the destination.
                let dir = (dp - rect.center()).normalized();
                if dir.length_sq() > 0.5 {
                    let inset = rect.shrink(14.0);
                    let mut t = f32::INFINITY;
                    if dir.x.abs() > 1e-4 {
                        let tx = if dir.x > 0.0 { (inset.right() - rect.center().x) / dir.x } else { (inset.left() - rect.center().x) / dir.x };
                        t = t.min(tx);
                    }
                    if dir.y.abs() > 1e-4 {
                        let ty = if dir.y > 0.0 { (inset.bottom() - rect.center().y) / dir.y } else { (inset.top() - rect.center().y) / dir.y };
                        t = t.min(ty);
                    }
                    if t.is_finite() && t > 0.0 {
                        let edge = rect.center() + dir * t;
                        let side = Vec2::new(-dir.y, dir.x);
                        let tip = edge + dir * 4.0;
                        let tri = vec![tip, edge - dir * 5.0 + side * 5.0, edge - dir * 5.0 - side * 5.0];
                        painter.add(egui::Shape::convex_polygon(tri, DEST_COLOR, Stroke::new(1.5, Color32::from_black_alpha(200))));
                    }
                }
            }
        }
        // the player or aircraft marker.
        if aviation {
            let arrow = [Vec2::new(0.0, -12.0), Vec2::new(8.5, 9.0), Vec2::new(0.0, 4.5), Vec2::new(-8.5, 9.0)];
            let points: Vec<_> = arrow.into_iter().map(|v| at + rotate(v, self.heading + view.angle)).collect();
            painter.add(egui::Shape::closed_line(points.clone(), Stroke::new(3.0, Color32::from_black_alpha(150))));
            for tri in [[0, 1, 2], [0, 2, 3]] {
                painter.add(egui::Shape::convex_polygon(tri.iter().map(|&k| points[k]).collect(), Color32::WHITE, Stroke::NONE));
            }
        } else {
            let arrow = [Vec2::new(0.0, -9.0), Vec2::new(6.5, 7.0), Vec2::new(0.0, 3.5), Vec2::new(-6.5, 7.0)];
            let points: Vec<_> = arrow.into_iter().map(|v| at + rotate(v, self.heading + view.angle)).collect();
            painter.circle_filled(at, 10.5, Color32::from_black_alpha(130));
            painter.circle_stroke(at, 10.5, Stroke::new(1.5, crate::menu::ACCENT));
            for tri in [[0, 1, 2], [0, 2, 3]] {
                painter.add(egui::Shape::convex_polygon(tri.iter().map(|&k| points[k]).collect(), Color32::WHITE, Stroke::NONE));
            }
        }
        // the scale bar on a chip.
        let meters = 2.0 * std::f64::consts::PI * geo::R * view.lat.cos() / (view.n * 256.0) * 60.0;
        let chip = egui::Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 22.0), Vec2::new(78.0, 26.0));
        painter.rect_filled(chip, crate::flightui::R_IN, Color32::from_black_alpha(150));
        painter.text(chip.center() - Vec2::new(0.0, 3.0), egui::Align2::CENTER_CENTER, distance_label(meters, crate::i18n::lang()), crate::menu::medium(12.5), Color32::WHITE);
        let a = Pos2::new(rect.center().x - 30.0, chip.bottom() - 6.0);
        painter.line_segment([a, a + Vec2::new(60.0, 0.0)], Stroke::new(1.5, Color32::WHITE));
        clip_to_disc(ctx, painter.layer_id(), first_shape, &outline);
        // a dark line outside, a bright hairline inside.
        painter.rect_stroke(rect, crate::flightui::R_HUD, Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Outside);
        painter.rect_stroke(rect, crate::flightui::R_HUD, Stroke::new(1.0, Color32::from_white_alpha(72)), egui::StrokeKind::Inside);
    }

    pub fn new(prefs: &crate::settings::Settings) -> Self {
        Self { visible: prefs.minimap, landing_guidance: prefs.landing_guidance, heading_up: prefs.minimap_heading_up, zoom: prefs.minimap_zoom,
            size: Vec2::splat(prefs.minimap_size), bottom: 65.0, top: 0.0, resize_start: 0.0, full: false, full_zoom: prefs.minimap_zoom.saturating_sub(2).max(3), full_center: None, full_scroll: 0.0, destination: None,
            plan: prefs.minimap_plan, maps: HashMap::new(), map_pending: Vec::new(), heading: 0.0, scroll: 0.0, tiles: HashMap::new(), pending: Vec::new(), generation: 0, frame: 0, runway_airports: Vec::new(), runway_center: None, runway_reach: 0.0,
            layers: crate::vfrmap::Layers { airports: prefs.map_airports, navaids: prefs.map_navaids }, tuned: Default::default(), trail: Default::default(), hits: Vec::new() }
    }

    pub fn set_destination(&mut self, lat: f64, lon: f64, name: Option<String>) {
        self.destination = Some(Destination { lat, lon, name });
    }

    /// What a click at `p` points at: a symbol of the chart under it, or the
    /// spot itself, named after an airport near it (radians).
    fn pick(&self, view: &View, p: Pos2) -> (f64, f64, Option<String>) {
        if let Some(t) = crate::vfrmap::hit_at(&self.hits, p, 12.0) {
            let (la, lo) = t.lat_lon();
            return (la.to_radians(), lo.to_radians(), Some(t.name()));
        }
        let (dlat, dlon) = view.unproject(p);
        (dlat, dlon, nearest_airport_name(dlat, dlon, view, p))
    }

    pub fn clear_destination(&mut self) {
        self.destination = None;
    }

    pub fn destination(&self) -> Option<&Destination> {
        self.destination.as_ref()
    }

    /// Opens or closes the full-size map; it opens on the aircraft.
    pub fn toggle_full(&mut self) {
        self.full = !self.full;
        self.full_center = None;
        self.full_scroll = 0.0;
    }

    fn poll(&mut self, ctx: &egui::Context, generation: u32) {
        let frame = self.frame;
        let mut done = Vec::new();
        self.map_pending.retain(|(id, rx)| match rx.try_recv() {
            Ok(result) => { done.push((*id, result)); false }
            Err(mpsc::TryRecvError::Disconnected) => false,
            Err(mpsc::TryRecvError::Empty) => true,
        });
        for (id, result) in done {
            let (texture, places) = result.map(|d| {
                let texture = ctx.load_texture(format!("minimap-plan-{id:?}"), egui::ColorImage::from_rgba_unmultiplied([256, 256], d.image.as_raw()), egui::TextureOptions::LINEAR);
                (Some(texture), d.places)
            }).unwrap_or_default();
            self.maps.insert(id, MapTile { texture, places, touched: frame, retry: Instant::now() + Duration::from_secs(30) });
        }
        // (the full-size map shows many more tiles at once)
        let (max_maps, max_tiles) = if self.full { (128, 256) } else { (48, 96) };
        while self.maps.len() > max_maps {
            let oldest = *self.maps.iter().min_by_key(|(_, t)| t.touched).unwrap().0;
            self.maps.remove(&oldest);
        }
        if generation != self.generation {
            self.tiles.clear();
            self.generation = generation;
        }
        let mut done = Vec::new();
        self.pending.retain(|(id, gen_id, rx)| match rx.try_recv() {
            Ok(result) => { if *gen_id == generation { done.push((*id, result)); } false }
            Err(mpsc::TryRecvError::Disconnected) => false,
            Err(mpsc::TryRecvError::Empty) => true,
        });
        for (id, result) in done {
            let texture = result.map(|img| {
                ctx.load_texture(format!("minimap-{id:?}-{generation}"),
                    egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()),
                    egui::TextureOptions::LINEAR)
            });
            self.tiles.insert(id, Tile { texture, touched: frame, retry: Instant::now() + Duration::from_secs(30) });
        }
        while self.tiles.len() > max_tiles {
            let oldest = *self.tiles.iter().min_by_key(|(_, t)| t.touched).unwrap().0;
            self.tiles.remove(&oldest);
        }
    }

    /// The map: the minimap in the corner, or the full-size map over the view.
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ctx: &egui::Context, src: &Arc<Sources>, lat: f64, lon: f64, heading: Option<f64>, info_open: bool, guide: Option<&crate::approach::Guide>, world: &crate::world::World, aviation: bool) {
        self.frame += 1;
        self.poll(ctx, src.google.generation());
        if let Some(h) = heading { self.heading = h; }
        if self.full {
            self.show_full(ctx, src, lat, lon, guide, world, aviation);
        } else {
            self.show_mini(ctx, src, lat, lon, info_open, guide, world, aviation);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_mini(&mut self, ctx: &egui::Context, src: &Arc<Sources>, lat: f64, lon: f64, info_open: bool, guide: Option<&crate::approach::Guide>, world: &crate::world::World, aviation: bool) {
        let reserved_width = if info_open { 426.0 } else { 44.0 };
        let limit = (ctx.content_rect().width() - reserved_width).min(ctx.content_rect().bottom() - self.bottom - 44.0 - self.top - 8.0).clamp(120.0, 640.0);
        let diameter = self.size.x.clamp(240.0, 640.0).min(limit);
        egui::Area::new(egui::Id::unique("minimap")).anchor(egui::Align2::LEFT_BOTTOM, [14.0, -self.bottom]).show(ctx, |ui| {
            ui.set_width(diameter);
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), egui::Sense::click_and_drag());
                let (center, radius) = (rect.center(), diameter * 0.5);
                let outline = Outline::new(rect);
                let over_disc = response.hover_pos().is_some_and(|p| outline.inside(p));
                if over_disc {
                    let scroll = ui.input_mut(|i| {
                        let s = i.smooth_scroll_delta.y;
                        i.smooth_scroll_delta = Vec2::ZERO;
                        s
                    });
                    self.scroll += scroll;
                    let steps = (self.scroll / 40.0).trunc() as i32;
                    self.scroll -= steps as f32 * 40.0;
                    self.zoom = (self.zoom as i32 + steps).clamp(3, 17) as u8;
                } else {
                    self.scroll = 0.0;
                }
                let painter = ui.painter_at(rect);
                let angle = if self.heading_up { -self.heading } else { 0.0 };
                let n = (1u32 << self.zoom) as f64;
                let view = View { rect, zoom: self.zoom, n, cx: geo::lon_to_merc_x(lon).rem_euclid(1.0) * n, cy: geo::lat_to_merc_y(lat) * n, angle, lat, lon, names: 8, loads: 4 };
                if response.secondary_clicked() && over_disc {
                    self.clear_destination();
                } else if response.clicked() && over_disc {
                    if let Some(p) = response.interact_pointer_pos() {
                        let (dlat, dlon, name) = self.pick(&view, p);
                        self.set_destination(dlat, dlon, name);
                    }
                }
                self.paint(ctx, src, &painter, &view, (lat, lon), guide, world, reserved(rect, angle), aviation, false);
                // north on a chip.
                let north = center + rotate(Vec2::new(0.0, -(radius - 18.0)), angle);
                painter.circle_filled(north, 11.0, Color32::from_black_alpha(160));
                painter.text(north, egui::Align2::CENTER_CENTER, "N", crate::menu::semibold(13.0), Color32::WHITE);
                // The full-size map, from the upper left corner.
                let expand = ui.interact(egui::Rect::from_center_size(rect.left_top() + Vec2::new(16.0, 16.0), Vec2::splat(24.0)), egui::Id::unique("minimap_full"), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(crate::t!("minimap.full_size_map"));
                if expand.clicked() { self.toggle_full(); }
                let hot = expand.hovered();
                painter.rect_filled(expand.rect, crate::menu::R_WIDGET, Color32::from_black_alpha(if hot { 200 } else { 150 }));
                expand_icon(&painter, expand.rect.center(), if hot { crate::flightui::LIT } else { Color32::from_white_alpha(210) });
                // Resize handle on the rim, upper right; the lower-left stays anchored.
                let knob = rect.right_top() + Vec2::new(-12.0, 12.0);
                let resize = ui.interact(egui::Rect::from_center_size(knob, Vec2::splat(18.0)), egui::Id::unique("minimap_resize"), egui::Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeNeSw)
                    .on_hover_text(crate::t!("minimap.drag_to_resize_the_map"));
                if resize.drag_started() { self.resize_start = diameter; }
                if resize.dragged() {
                    let d = resize.total_drag_delta().unwrap_or_default();
                    let size = (self.resize_start + (d.x - d.y) * 0.5).clamp(240.0f32.min(limit), limit);
                    self.size = Vec2::splat(size);
                }
                // the resize grip, lit under the pointer.
                let hot = resize.hovered() || resize.dragged();
                let col = if hot { crate::flightui::LIT } else { Color32::from_white_alpha(150) };
                for k in [0.0, 4.5, 9.0] {
                    painter.line_segment([knob + Vec2::new(-6.0 + k, -6.0), knob + Vec2::new(6.0, 6.0 - k)], Stroke::new(1.4, col));
                }
            });
        });
    }

    /// The full-size map over the whole view, north up: drag to move it, the
    /// wheel zooms about the pointer, and it follows the player until moved.
    #[allow(clippy::too_many_arguments)]
    fn show_full(&mut self, ctx: &egui::Context, src: &Arc<Sources>, lat: f64, lon: f64, guide: Option<&crate::approach::Guide>, world: &crate::world::World, aviation: bool) {
        let screen = ctx.content_rect();
        let margin = if screen.width() < 900.0 { 12.0 } else { 28.0 };
        let area = screen.shrink(margin);
        egui::Area::new(egui::Id::unique("full_map")).order(egui::Order::Foreground).fixed_pos(area.min).show(ctx, |ui| {
            // the view behind, dimmed.
            ui.painter().with_clip_rect(screen).rect_filled(screen, 0.0, Color32::from_black_alpha(130));
            let (rect, response) = ui.allocate_exact_size(area.size(), egui::Sense::click_and_drag());
            let aircraft = glam::DVec2::new(geo::lon_to_merc_x(lon).rem_euclid(1.0), geo::lat_to_merc_y(lat));
            let mut center = self.full_center.unwrap_or(aircraft);
            let mut zoom = self.full_zoom.clamp(3, 17);
            let scale = |z: u8| 256.0 * (1u32 << z) as f64;
            if response.dragged() {
                let d = response.drag_delta();
                if d != Vec2::ZERO {
                    center -= glam::DVec2::new(d.x as f64, d.y as f64) / scale(zoom);
                    self.full_center = Some(center);
                }
            }
            // The wheel: zoom about the point under the pointer.
            let zoom_at = |zoom: &mut u8, center: &mut glam::DVec2, steps: i32, at: Pos2| -> bool {
                let off = at - rect.center();
                let (c, z) = zoom_about(*center, *zoom, steps, glam::DVec2::new(off.x as f64, off.y as f64));
                let changed = z != *zoom;
                (*center, *zoom) = (c, z);
                changed
            };
            if let Some(p) = response.hover_pos() {
                let scroll = ui.input_mut(|i| {
                    let s = i.smooth_scroll_delta.y;
                    i.smooth_scroll_delta = Vec2::ZERO;
                    s
                });
                self.full_scroll += scroll;
                let steps = (self.full_scroll / 40.0).trunc() as i32;
                self.full_scroll -= steps as f32 * 40.0;
                // About the aircraft while following it, so it stays in the middle.
                let at = if self.full_center.is_none() { rect.center() } else { p };
                if steps != 0 && zoom_at(&mut zoom, &mut center, steps, at) && self.full_center.is_some() {
                    self.full_center = Some(center);
                }
            } else {
                self.full_scroll = 0.0;
            }
            center.x = center.x.rem_euclid(1.0);
            center.y = center.y.clamp(geo::lat_to_merc_y(85f64.to_radians()), geo::lat_to_merc_y(-85f64.to_radians()));
            if self.full_center.is_some() { self.full_center = Some(center); }
            let n = (1u32 << zoom) as f64;
            let view = View { rect, zoom, n, cx: center.x * n, cy: center.y * n, angle: 0.0, lat: geo::merc_y_to_lat(center.y), lon: center.x * 2.0 * std::f64::consts::PI - std::f64::consts::PI, names: 40, loads: 8 };
            if response.secondary_clicked() {
                self.clear_destination();
            } else if response.clicked() {
                if let Some(p) = response.interact_pointer_pos() {
                    let (dlat, dlon, name) = self.pick(&view, p);
                    self.set_destination(dlat, dlon, name);
                }
            }
            let painter = ui.painter_at(rect);
            // the toolbar, the hint and the aircraft keep names off them.
            let bar = egui::Rect::from_min_max(Pos2::new(rect.right() - 340.0, rect.top()), Pos2::new(rect.right(), rect.top() + 56.0));
            let title = egui::Rect::from_min_size(rect.left_top(), Vec2::new(300.0, 56.0));
            let hint = egui::Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 44.0), Pos2::new(rect.left() + 420.0, rect.bottom()));
            let scale_chip = egui::Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 22.0), Vec2::new(84.0, 32.0));
            let plane_at = view.project(aircraft.x, aircraft.y);
            self.paint(ctx, src, &painter, &view, (lat, lon), guide, world, vec![bar, title, hint, scale_chip, egui::Rect::from_center_size(plane_at, Vec2::splat(38.0))], aviation, true);
            // what the pointer is on.
            let hover = response.hover_pos().filter(|_| !response.dragged()).and_then(|p| crate::vfrmap::hit_at(&self.hits, p, 12.0).map(|t| (p, t)));
            if let Some((p, t)) = hover {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                crate::vfrmap::card(&painter, p, t, (lat.to_degrees(), lon.to_degrees()), rect);
            }
            // the title.
            let name = painter.layout_no_wrap(crate::t!("minimap.map").to_string(), crate::menu::semibold(18.0), Color32::WHITE);
            let chip = egui::Rect::from_min_size(rect.left_top() + Vec2::new(14.0, 14.0), name.size() + Vec2::new(20.0, 10.0));
            painter.rect_filled(chip, crate::menu::R_WIDGET, Color32::from_black_alpha(170));
            painter.galley(chip.min + Vec2::new(10.0, 5.0), name, Color32::WHITE);
            // how to work it.
            let help = painter.layout_no_wrap(crate::t!("minimap.full_map_hint").to_string(), crate::menu::caption(), Color32::from_white_alpha(220));
            let chip = egui::Rect::from_min_size(Pos2::new(rect.left() + 14.0, rect.bottom() - 14.0 - help.size().y - 10.0), help.size() + Vec2::new(20.0, 10.0));
            painter.rect_filled(chip, crate::menu::R_WIDGET, Color32::from_black_alpha(160));
            painter.galley(chip.min + Vec2::new(10.0, 5.0), help, Color32::WHITE);
            // zoom, back to the aircraft, close.
            let mut x = rect.right() - 14.0;
            let button = |ui: &mut egui::Ui, x: &mut f32, w: f32, id: &str, tip: &str, icon: &dyn Fn(&egui::Painter, egui::Rect, Color32)| {
                let r = egui::Rect::from_min_size(Pos2::new(*x - w, rect.top() + 14.0), Vec2::new(w, 32.0));
                *x -= w + 6.0;
                let b = ui.interact(r, egui::Id::unique(("full_map_btn", id)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip.to_string());
                let hot = b.hovered();
                painter.rect_filled(r, crate::menu::R_WIDGET, if hot { Color32::from_rgba_unmultiplied(42, 50, 63, 235) } else { Color32::from_black_alpha(175) });
                icon(&painter, r, if hot { crate::flightui::LIT } else { Color32::WHITE });
                b.clicked()
            };
            let stroke = |c: Color32| Stroke::new(2.0, c);
            if button(ui, &mut x, 32.0, "close", crate::t!("minimap.close_map"), &|p, r, c| {
                let (m, s) = (r.center(), 6.0);
                p.line_segment([m + Vec2::new(-s, -s), m + Vec2::new(s, s)], stroke(c));
                p.line_segment([m + Vec2::new(-s, s), m + Vec2::new(s, -s)], stroke(c));
            }) {
                self.full = false;
            }
            x -= 8.0;
            if button(ui, &mut x, 32.0, "in", crate::t!("minimap.zoom_in"), &|p, r, c| {
                let m = r.center();
                p.line_segment([m - Vec2::new(6.0, 0.0), m + Vec2::new(6.0, 0.0)], stroke(c));
                p.line_segment([m - Vec2::new(0.0, 6.0), m + Vec2::new(0.0, 6.0)], stroke(c));
            }) && zoom_at(&mut zoom, &mut center, 1, rect.center()) && self.full_center.is_some() {
                self.full_center = Some(center);
            }
            if button(ui, &mut x, 32.0, "out", crate::t!("minimap.zoom_out"), &|p, r, c| {
                let m = r.center();
                p.line_segment([m - Vec2::new(6.0, 0.0), m + Vec2::new(6.0, 0.0)], stroke(c));
            }) && zoom_at(&mut zoom, &mut center, -1, rect.center()) && self.full_center.is_some() {
                self.full_center = Some(center);
            }
            x -= 8.0;
            let centre_tip = if aviation { crate::t!("minimap.back_to_the_aircraft") } else { crate::t!("minimap.back_to_player") };
            if self.full_center.is_some() && button(ui, &mut x, 32.0, "centre", centre_tip, &|p, r, c| {
                let m = r.center();
                p.circle_stroke(m, 6.0, stroke(c));
                p.circle_filled(m, 2.0, c);
                for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                    p.line_segment([m + d * 8.0, m + d * 11.0], stroke(c));
                }
            }) {
                self.full_center = None;
            }
            if self.destination.is_some() && button(ui, &mut x, 32.0, "clear_dest", crate::t!("minimap.clear_destination"), &|p, r, c| {
                let m = r.center();
                let s = 6.0;
                p.add(egui::Shape::closed_line(vec![m + Vec2::new(0.0, -s), m + Vec2::new(s, 0.0), m + Vec2::new(0.0, s), m + Vec2::new(-s, 0.0)], stroke(DEST_COLOR)));
                p.line_segment([m + Vec2::new(-4.5, -4.5), m + Vec2::new(4.5, 4.5)], stroke(c));
            }) {
                self.clear_destination();
            }
            x -= 8.0;
            // the chart's layers, lit when shown.
            let on = |b: bool, c: Color32| if b { c } else { Color32::from_white_alpha(90) };
            if aviation {
                let navaids = self.layers.navaids;
                if button(ui, &mut x, 32.0, "navaids", crate::t!("vfr.radio_beacons"), &|p, r, c| crate::vfrmap::navaid_icon(p, r.center(), on(navaids, if c == Color32::WHITE { crate::vfrmap::VOR } else { c }))) {
                    self.layers.navaids = !navaids;
                }
            }
            let airports = self.layers.airports;
            if button(ui, &mut x, 32.0, "airports", crate::t!("vfr.airports"), &|p, r, c| crate::vfrmap::airport_icon(p, r.center(), on(airports, if c == Color32::WHITE { crate::vfrmap::TOWERED } else { c }))) {
                self.layers.airports = !airports;
            }
            self.full_zoom = zoom;
            // north is up.
            let north = Pos2::new(rect.right() - 30.0, rect.top() + 76.0);
            painter.circle_filled(north, 13.0, Color32::from_black_alpha(160));
            painter.text(north, egui::Align2::CENTER_CENTER, "N", crate::menu::semibold(14.0), Color32::WHITE);
        });
    }
}

/// Zoom by `steps` keeping the point `off` pixels from the middle where it is:
/// the new centre (Web Mercator, 0..1) and zoom.
fn zoom_about(center: glam::DVec2, zoom: u8, steps: i32, off: glam::DVec2) -> (glam::DVec2, u8) {
    let next = (zoom as i32 + steps).clamp(3, 17) as u8;
    let scale = |z: u8| 256.0 * (1u32 << z) as f64;
    let point = center + off / scale(zoom);
    (point - off / scale(next), next)
}

/// Four corner marks pointing out: "make it bigger".
fn expand_icon(painter: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = c + Vec2::new(sx * 6.5, sy * 6.5);
        painter.line_segment([corner, corner - Vec2::new(sx * 4.0, 0.0)], s);
        painter.line_segment([corner, corner - Vec2::new(0.0, sy * 4.0)], s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_uses_localized_decimal_separator() {
        assert_eq!(distance_label(1500.0, crate::i18n::Lang::parse("fr").unwrap()), "1,5 km");
        assert_eq!(distance_label(1500.0, crate::i18n::Lang::EN), "1.5 km");
        assert_eq!(distance_label(250.0, crate::i18n::Lang::parse("fr").unwrap()), "250 m");
    }
    #[test]
    fn heading_up_keeps_aircraft_up_and_rotates_east_up() {
        let heading = std::f64::consts::FRAC_PI_2;
        assert!((rotate(Vec2::new(10.0, 0.0), -heading) - Vec2::new(0.0, -10.0)).length() < 1e-5);
        assert!((rotate(Vec2::new(0.0, -10.0), heading - heading) - Vec2::new(0.0, -10.0)).length() < 1e-5);
    }
    #[test]
    fn full_map_zooms_about_the_pointer() {
        let (c, off) = (glam::DVec2::new(0.16, 0.39), glam::DVec2::new(300.0, -120.0));
        let (c2, z2) = zoom_about(c, 10, -3, off);
        assert_eq!(z2, 7);
        // the same spot under the pointer before and after
        let at = |c: glam::DVec2, z: u8| c + off / (256.0 * (1u32 << z) as f64);
        assert!((at(c, 10) - at(c2, z2)).length() < 1e-12);
        // clamped at the ends, and then nothing moves
        assert_eq!(zoom_about(c, 17, 2, off), (c, 17));
    }
    #[test]
    fn antimeridian_uses_nearest_copy() {
        assert!((tile_offset(0.0, 255.75, 256.0) - 0.25).abs() < 1e-8);
        assert!((tile_offset(255.0, 0.25, 256.0) + 1.25).abs() < 1e-8);
    }
    #[test]
    fn unproject_inverts_project_with_rotation() {
        let rect = egui::Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(300.0));
        let (lat, lon) = (48.8566_f64.to_radians(), 2.3522_f64.to_radians());
        let n = (1u32 << 10) as f64;
        let view = View { rect, zoom: 10, n, cx: geo::lon_to_merc_x(lon) * n, cy: geo::lat_to_merc_y(lat) * n, angle: -0.7, lat, lon, names: 8, loads: 4 };
        let (tlat, tlon) = (48.92_f64.to_radians(), 2.45_f64.to_radians());
        let p = view.project(geo::lon_to_merc_x(tlon).rem_euclid(1.0), geo::lat_to_merc_y(tlat));
        let (ulat, ulon) = view.unproject(p);
        assert!((ulat - tlat).abs() < 1e-4);
        assert!((ulon - tlon).abs() < 1e-4);
    }
}
