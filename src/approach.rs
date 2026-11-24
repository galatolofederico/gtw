//! Optional visual aid for the nearby airfield's active runway.
use crate::{airports, geo, traffic};
use egui::{Color32, Pos2, Stroke, Vec2};
use glam::{DVec3, Mat4};

pub struct Guide {
    pub name: String,
    threshold: DVec3,
    direction: DVec3,
    up: DVec3,
    aim: f64,
    width: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrival_gates_extend_before_threshold_and_follow_three_degree_slope() {
        let guide = Guide {
            name: "TEST 09".into(), threshold: DVec3::X * (geo::R + 100.0),
            direction: DVec3::Y, up: DVec3::X, aim: 300.0, width: 35.0,
        };
        for distance in [250.0, 5250.0, 10250.0] {
            let gate = guide.center(distance);
            assert!((gate - guide.threshold).dot(guide.direction) < 0.0);
            let height = gate.length() - guide.threshold.length();
            assert!((height - (distance + guide.aim) * 3.0_f64.to_radians().tan()).abs() < 1e-6);
        }
    }
}

impl Guide {
    pub fn new(g: &airports::Geo, runway: usize, end: usize) -> Self {
        let r = &g.runways[runway];
        let (threshold, direction) = r.end(end);
        Self {
            name: format!("{} · {}", g.apt().ident, r.ident[end]),
            threshold: g.world(threshold, g.h(threshold) as f64),
            direction: (g.e * direction.x as f64 + g.n * direction.y as f64).normalize(),
            up: g.u,
            aim: traffic::aim_dist(r.len) as f64,
            width: (r.width as f64 * 0.75).max(35.0),
        }
    }

    fn center(&self, distance: f64) -> DVec3 {
        let ground = (self.threshold - self.direction * distance).normalize() * self.threshold.length();
        ground + ground.normalize() * ((distance + self.aim) * 3.0_f64.to_radians().tan())
    }

    pub fn draw_world(&self, ctx: &egui::Context, vp: Mat4, camera: DVec3, aircraft: DVec3) {
        // Only the arrival side gets gates, never the departure/rollout side.
        let along = (self.threshold - aircraft).dot(self.direction);
        if !(0.0..=18_000.0).contains(&along) { return; }
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        let project = |point: DVec3| -> Option<Pos2> {
            let c = vp * (point - camera).as_vec3().extend(1.0);
            if c.w < 1.0 || c.z < 0.0 { return None; }
            Some(Pos2::new(screen.left() + (c.x / c.w * 0.5 + 0.5) * screen.width(), screen.top() + (0.5 - c.y / c.w * 0.5) * screen.height()))
        };
        let right = self.direction.cross(self.up).normalize();
        for i in (0..=20).rev() {
            let distance = 250.0 + i as f64 * 500.0;
            let center = self.center(distance);
            let half_width = self.width + distance * 0.012;
            let half_height = 12.0 + distance * 0.004;
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| project(center + right * x * half_width + self.up * y * half_height));
            let [Some(a), Some(b), Some(c), Some(d)] = corners else { continue; };
            let color = Color32::from_rgba_unmultiplied(255, 145, 50, 190);
            painter.add(egui::Shape::closed_line(vec![a, b, c, d], Stroke::new(1.5, color)));
        }
        if let Some(at) = project(self.center(250.0)) {
            painter.text(at + Vec2::new(0.0, -24.0), egui::Align2::CENTER_BOTTOM,
                crate::t!("approach.visual_approach_3", self.name),
                egui::FontId::proportional(13.0), Color32::from_rgb(255, 145, 50));
        }
    }

    pub fn draw_map(&self, painter: &egui::Painter, rect: egui::Rect, cx: f64, cy: f64, n: f64, angle: f64) {
        let project = |point: DVec3| {
            let (lat, lon, _) = geo::pos_to_lla(point);
            let x = geo::lon_to_merc_x(lon) * n;
            let y = geo::lat_to_merc_y(lat) * n;
            let delta = Vec2::new(((x - cx + n * 0.5).rem_euclid(n) - n * 0.5) as f32, (y - cy) as f32) * 256.0;
            let (s, c) = (angle as f32).sin_cos();
            rect.center() + Vec2::new(c * delta.x - s * delta.y, s * delta.x + c * delta.y)
        };
        let end = project(self.threshold);
        let start = project(self.center(10_250.0));
        let delta = end - start;
        if delta.length() < 12.0 { return; }
        let direction = delta.normalized();
        let side = Vec2::new(-direction.y, direction.x);
        let color = Color32::from_rgb(255, 145, 50);
        painter.add(egui::Shape::convex_polygon(vec![end - side * 3.0, end + side * 3.0, start + side * 12.0, start - side * 12.0], color.gamma_multiply(0.15), Stroke::new(1.0, color)));
        for fraction in [0.25, 0.5, 0.75] {
            let tip = start + delta * fraction;
            painter.line_segment([tip - direction * 7.0 + side * 4.0, tip], Stroke::new(2.0, color));
            painter.line_segment([tip - direction * 7.0 - side * 4.0, tip], Stroke::new(2.0, color));
        }
        let label = start + Vec2::new(0.0, -16.0);
        if rect.shrink(35.0).contains(label) {
            painter.text(label, egui::Align2::CENTER_BOTTOM, &self.name, egui::FontId::proportional(12.0), color);
        }
    }
}
