//! the main menu: the wordmark over the world map, settings and multiplayer tiles.
//! Quit sits small in the bottom left corner. Behind it the renderer draws
//! space alone (the menu camera looks away from the planet, see `frame`).
use crate::menu::{self, Screen, ACCENT, DIM, FAINT, TEXT};
use crate::padui::{self, Hint};
use crate::{save, settings_ui, t, tr_en, Game};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke};

/// The picture behind the WORLD tile, 4:3 at a little over twice the tile's
/// size: the game's own globe (sky.wgsl `globe_*`, live clouds) rendered
/// from 7600 km over the Caribbean (`--menu-view 24,-66,7600000 --time 8.35
/// --day 80`, exposure 0.3, a 3840x2160 frame with the menu hidden), cropped
/// so the planet rises off the right of the tile with the morning terminator
/// and the US's city lights along its left edge, the dark space to its left
/// left for the title. Swap the file to change it; nothing else depends on
/// its size beyond the aspect (it is cropped to fill the tile).
pub const GLOBE_PICTURE: &[u8] = include_bytes!("../assets/ui/globe.jpg");

/// What a tile does when pressed.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Tile {
    Fly,
    Settings,
    /// spawn at the place last started.
    Again,
    /// pick the resume snapshot back up where it was left.
    Continue,
    /// the settings' Multiplayer section
    Multiplayer,
}

impl Game {
    /// The main menu (see the module docs).
    pub(crate) fn main_menu(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let globe = self.picture(ctx, GLOBE_PICTURE);
        let mut press: Option<Tile> = None;
        let mut quit = false;
        let again = self.last_start_label();
        let resume = self.save.as_ref().map(save::summary);
        let rise = entrance(ctx);
        egui::Area::new(egui::Id::unique("main_menu")).order(egui::Order::Background).fixed_pos(screen.min).show(ctx, |ui| {
            let p = ui.painter().clone();
            // Darken space towards the foot of the screen, for depth.
            gradient(&p, Rect::from_min_max(pos2(screen.left(), screen.center().y), screen.max), Color32::TRANSPARENT, Color32::from_rgba_premultiplied(4, 6, 10, 200));

            // The tiles: WORLD map hero on the left, Settings + Multiplayer beside it.
            let gap = 18.0;
            let (fw, fh, sw, sh) = (680.0, 440.0, 320.0, 440.0);
            let total = fw + sw + gap;
            let top = 104.0;
            let k = ((screen.width() - 48.0) / total).min((screen.height() - top - 84.0) / fh).min(1.05).max(0.45);
            let (fw, fh, sw, sh, gap) = (fw * k, fh * k, sw * k, sh * k, gap * k);
            let cy = top + (screen.height() - top - 60.0) * 0.5;
            let x0 = screen.center().x - (fw + sw + gap) * 0.5;
            // the tiles rise in as the menu opens.
            let lift = |r: Rect, i: usize| r.translate(vec2(0.0, 26.0 * (1.0 - rise(i))));
            let fly = lift(Rect::from_min_size(pos2(x0, cy - fh * 0.5), vec2(fw, fh)), 0);
            let bar_h = 46.0;
            let set = lift(Rect::from_min_size(pos2(x0 + fw + gap, cy - sh * 0.5), vec2(sw, sh - bar_h - gap)), 1);
            let mpb = lift(Rect::from_min_size(pos2(set.left(), set.bottom() + gap), vec2(sw, bar_h)), 2);
            let mut pw = p.clone();
            pw.set_opacity(rise(0));
            wordmark(&pw, pos2(x0, fly.top() - 26.0 * k.max(0.7)), x0 + fw + sw + gap, k);

            let r = ui.interact(fly, ui.scope_id().with("tile_fly"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.fly")));
            self.pad_home_focus(&r);
            let mut pf = p.clone();
            pf.set_opacity(rise(0));
            fly_tile(&pf, fly, &r, k, &globe);
            if r.clicked() {
                press = Some(Tile::Fly);
            }
            let r = ui.interact(set, ui.scope_id().with("tile_set"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.settings")));
            let mut ps = p.clone();
            ps.set_opacity(rise(1));
            settings_tile(&ps, set, &r, k);
            if r.clicked() {
                press = Some(Tile::Settings);
            }
            let r = ui.interact(mpb, ui.scope_id().with("tile_mp"), Sense::click());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.multiplayer")));
            let mut pm = p.clone();
            pm.set_opacity(rise(2));
            let (col, status) = self.mp_short();
            mp_bar(&pm, mpb, &r, &status, col);
            if r.clicked() {
                press = Some(Tile::Multiplayer);
            }

            // Under the map tile: the resume snapshot first, then the last start,
            // one press away each.
            let mut bar_y = fly.bottom() + 14.0 * k;
            if let Some(where_) = &resume {
                let b = Rect::from_min_size(pos2(fly.left(), bar_y), vec2(fly.width(), 46.0));
                let r = ui.interact(b, ui.scope_id().with("continue"), Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.continue")));
                let mut pc = p.clone();
                pc.set_opacity(rise(2));
                continue_button(&pc, b, &r, where_);
                if r.clicked() {
                    press = Some(Tile::Continue);
                }
                bar_y += 46.0 + 8.0 * k;
            }
            if let Some(place) = &again {
                let b = Rect::from_min_size(pos2(fly.left(), bar_y), vec2(fly.width(), 46.0));
                let r = ui.interact(b, ui.scope_id().with("again"), Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.fly_again")));
                let mut pg = p.clone();
                pg.set_opacity(rise(2));
                again_button(&pg, b, &r, place);
                if r.clicked() {
                    press = Some(Tile::Again);
                }
            }

            // The foot: Quit at the left, the gamepad's buttons (or a word on the keys) in the middle.
            let foot_y = screen.bottom() - 34.0;
            let q = Rect::from_min_size(pos2(screen.left() + 24.0, foot_y - 17.0), vec2(104.0, 34.0));
            let qr = ui.interact(q, ui.scope_id().with("quit"), Sense::click());
            qr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t!("menu.quit")));
            let lit = menu::ease(ctx, qr.id, qr.hovered() || qr.has_focus());
            if lit > 0.0 {
                p.rect_filled(q, menu::R_WIDGET, Color32::from_white_alpha((14.0 * lit) as u8));
            }
            let qc = menu::mix(DIM, Color32::WHITE, lit);
            power_icon(&p, pos2(q.left() + 18.0, q.center().y), 14.0, qc);
            p.text(pos2(q.left() + 34.0, q.center().y), Align2::LEFT_CENTER, t!("menu.quit"), menu::medium(15.0), qc);
            if qr.clicked() {
                quit = true;
            }
            if padui::active() {
                let mut f = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(screen.center().x, foot_y), vec2(360.0, 28.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                padui::prompts(&mut f, &[("Dpad", tr_en("move")), ("A", tr_en("press"))], 20.0, 12.5, DIM);
            }
            p.text(pos2(screen.right() - 24.0, foot_y), Align2::RIGHT_CENTER, t!("menu.globe_esri_maxar_earthstar_geographics"), FontId::proportional(11.5), FAINT);
        });
        match press {
            Some(Tile::Fly) => self.menu.screen = Screen::Fly,
            Some(Tile::Settings) => {
                self.menu.set_back = Screen::Main;
                self.menu.screen = Screen::Settings;
            }
            Some(Tile::Multiplayer) => self.open_settings(settings_ui::Section::Multiplayer, Screen::Main),
            Some(Tile::Again) => {
                // over the map, which shows it preparing.
                self.restore_last_start();
                self.menu.screen = Screen::Fly;
                self.begin();
            }
            Some(Tile::Continue) => {
                if !self.resume_save() {
                    self.status = t!("save.resume_failed").into();
                    self.status_until = self.real + 4.0;
                }
            }
            None => {}
        }
        if press.is_some() {
            self.pad_home = true;
        }
        if quit {
            self.quit = true;
        }
    }
}

/// how far each tile has risen since the menu opened, 0..1.
fn entrance(ctx: &egui::Context) -> impl Fn(usize) -> f32 {
    let now = ctx.input(|i| i.time);
    let id = egui::Id::unique("main_menu_entrance");
    let (start, seen) = ctx.data(|d| d.get_temp::<(f64, f64)>(id)).unwrap_or((now, f64::NEG_INFINITY));
    // away for a moment: play it again.
    let start = if now - seen > 0.3 { now } else { start };
    ctx.data_mut(|d| d.insert_temp(id, (start, now)));
    let t = (now - start) as f32;
    if t < 1.0 {
        ctx.request_repaint();
    }
    move |i: usize| {
        let x = ((t - 0.07 * i as f32) / 0.55).clamp(0.0, 1.0);
        1.0 - (1.0 - x).powi(4)
    }
}

/// the spawn again bar under the world tile.
fn again_button(p: &Painter, r: Rect, resp: &egui::Response, place: &str) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    p.rect_filled(r, menu::R_WIDGET, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(26, 32, 42, 238), lit));
    p.rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
    // a circular arrow.
    let c = pos2(r.left() + 24.0, r.center().y);
    let col = menu::mix(TEXT, ACCENT, lit);
    let pts: Vec<Pos2> = (0..=18).map(|i| {
        let a = -0.4 + i as f32 / 18.0 * 4.9;
        c + vec2(a.cos(), a.sin()) * 7.0
    }).collect();
    let end = *pts.last().unwrap();
    p.line(pts, Stroke::new(1.8, col));
    p.add(egui::Shape::convex_polygon(vec![end + vec2(3.5, -3.5), end + vec2(4.0, 3.0), end + vec2(-2.5, 1.5)], col, Stroke::NONE));
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, t!("menu.fly_again"), menu::label(), menu::mix(TEXT, Color32::WHITE, lit));
    let g = p.layout_no_wrap(place.to_string(), menu::caption(), DIM);
    p.galley(pos2(r.right() - 16.0 - g.size().x, r.center().y - g.size().y * 0.5), g, DIM);
}

/// The resume bar under fly: pick up where the last session left off.
fn continue_button(p: &Painter, r: Rect, resp: &egui::Response, where_: &str) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    p.rect_filled(r, menu::R_WIDGET, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(26, 32, 42, 238), lit));
    p.rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
    let col = menu::mix(TEXT, ACCENT, lit);
    // a play mark in a ring.
    let c = pos2(r.left() + 24.0, r.center().y);
    p.circle_stroke(c, 8.0, Stroke::new(1.6, col));
    p.add(egui::Shape::convex_polygon(vec![c + vec2(-2.2, -4.2), c + vec2(-2.2, 4.2), c + vec2(4.2, 0.0)], col, Stroke::NONE));
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, t!("menu.continue"), menu::label(), menu::mix(TEXT, Color32::WHITE, lit));
    let g = p.layout_no_wrap(where_.to_string(), menu::caption(), DIM);
    p.galley(pos2(r.right() - 16.0 - g.size().x, r.center().y - g.size().y * 0.5), g, DIM);
}

/// Multiplayer: a slim bar under the Settings tile, its two people and
/// where it stands (a coloured dot and a few words).
fn mp_bar(p: &Painter, r: Rect, resp: &egui::Response, status: &str, col: Color32) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    p.rect_filled(r, menu::R_WIDGET, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(26, 32, 42, 238), lit));
    p.rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
    let ic = menu::mix(TEXT, ACCENT, lit);
    settings_ui::Section::Multiplayer.icon(p, pos2(r.left() + 24.0, r.center().y), 18.0, ic);
    p.text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, t!("menu.multiplayer"), menu::label(), menu::mix(TEXT, Color32::WHITE, lit));
    let g = p.layout_no_wrap(status.to_string(), menu::caption(), DIM);
    // (a long status, an error say, is cut to the room left)
    let room = r.width() - 44.0 - 120.0;
    let x = r.right() - 14.0 - g.size().x.min(room);
    let clip = p.with_clip_rect(Rect::from_min_max(pos2(x, r.top()), pos2(r.right() - 14.0, r.bottom())));
    clip.galley(pos2(x, r.center().y - g.size().y * 0.5), g, DIM);
    p.circle_filled(pos2(x - 10.0, r.center().y), 3.5, col);
}

/// the wordmark at `at`, a hairline running on to `right`.
fn wordmark(p: &Painter, at: Pos2, right: f32, k: f32) {
    let size = 60.0 * k.max(0.7);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |col| egui::TextFormat { font_id: menu::bold(size), color: col, extra_letter_spacing: 3.0 * k, ..Default::default() };
    job.append("GT", 0.0, fmt(Color32::WHITE));
    job.append("W", 0.0, fmt(ACCENT));
    let g = p.layout_job(job);
    let r = Align2::LEFT_BOTTOM.anchor_size(at, g.size());
    let w = g.size().x;
    p.galley(r.min, g, Color32::WHITE);
    // a short accent bar over the G, and a hairline from the end of the word to the row's edge.
    p.rect_filled(Rect::from_min_size(pos2(r.left() + 2.0, r.top() - 6.0), vec2(34.0 * k.max(0.7), 3.0)), 1, ACCENT);
    let y = r.center().y + 2.0;
    if right - (r.left() + w + 24.0) > 40.0 {
        p.hline(r.left() + w + 20.0..=right, y, Stroke::new(1.0, Color32::from_white_alpha(46)));
    }
}

/// the frame every tile shares, its border lit on hover.
fn tile_frame(p: &Painter, r: Rect, lit: f32, fill: bool) {
    if fill {
        p.rect_filled(r, menu::R_PANEL, menu::mix(Color32::from_rgba_premultiplied(12, 16, 22, 226), Color32::from_rgba_premultiplied(22, 28, 37, 236), lit));
    }
    p.rect_stroke(r, menu::R_PANEL, Stroke::new(1.0 + lit, menu::mix(Color32::from_white_alpha(26), ACCENT, lit)), egui::StrokeKind::Inside);
}

/// a side tile's name and the line under it.
fn caption(p: &Painter, r: Rect, k: f32, lit: f32, title: &str, sub: &str) {
    let y = r.bottom() - 74.0 * k.max(0.7);
    spaced(p, pos2(r.left() + 24.0 * k, y), Align2::LEFT_BOTTOM, &title.to_uppercase(), menu::bold(30.0 * k.max(0.7)), menu::mix(TEXT, Color32::WHITE, lit), 2.5 * k);
    p.text(pos2(r.left() + 25.0 * k, y + 8.0), Align2::LEFT_TOP, sub, menu::label(), menu::mix(DIM, TEXT, lit));
}

/// the settings tile: a cog that turns on hover.
fn settings_tile(p: &Painter, r: Rect, resp: &egui::Response, k: f32) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    tile_frame(p, r, lit, true);
    let c = pos2(r.center().x, r.top() + r.height() * 0.38);
    p.circle_stroke(c, 72.0 * k, Stroke::new(1.0, Color32::from_white_alpha((14.0 + 16.0 * lit) as u8)));
    gear_icon_turned(p, c, 92.0 * k, menu::mix(Color32::from_rgb(200, 207, 218), Color32::WHITE, lit), lit * std::f32::consts::TAU / 16.0);
    caption(p, r, k, lit, t!("menu.settings"), t!("menu.graphics_sound_controls"));
}

/// FLY: the globe picture filling it (easing in a little when hovered),
/// shades along the foot and the left for the words, and the word.
fn fly_tile(p: &Painter, r: Rect, resp: &egui::Response, k: f32, globe: &egui::TextureHandle) {
    let lit = menu::ease(p.ctx(), resp.id, resp.hovered() || resp.has_focus());
    let zoom = p.ctx().animate_bool_with_time(resp.id.with("zoom"), lit > 0.5, 0.9);
    p.rect_filled(r, menu::R_PANEL, Color32::BLACK);
    // Fill the tile, cropping the picture to its aspect.
    let [iw, ih] = globe.size().map(|v| v as f32);
    let (ta, ia) = (r.width() / r.height(), iw / ih);
    let uv = if ia > ta {
        let f = ta / ia;
        Rect::from_min_max(pos2(0.5 - f * 0.5, 0.0), pos2(0.5 + f * 0.5, 1.0))
    } else {
        let f = ia / ta;
        Rect::from_min_max(pos2(0.0, 0.5 - f * 0.5), pos2(1.0, 0.5 + f * 0.5))
    };
    // A slow push in toward the planet's lit face on hover (eased).
    let z = 1.0 - 0.035 * (zoom * zoom * (3.0 - 2.0 * zoom));
    let at = uv.lerp_inside(vec2(0.72, 0.42));
    let uv = Rect::from_min_max(at + (uv.min - at) * z, at + (uv.max - at) * z);
    p.image(globe.id(), r.shrink(1.0), uv, menu::mix(Color32::from_gray(222), Color32::WHITE, lit));
    // Shades for the words: across from the left, and up from the foot.
    let inner = r.shrink(1.0);
    hgradient(p, Rect::from_min_max(inner.min, pos2(inner.left() + inner.width() * 0.5, inner.bottom())), Color32::from_black_alpha(130), Color32::TRANSPARENT);
    gradient(p, Rect::from_min_max(pos2(inner.left(), inner.top() + inner.height() * 0.5), inner.max), Color32::TRANSPARENT, Color32::from_black_alpha(180));
    tile_frame(p, r, lit, false);
    let x = r.left() + 32.0 * k;
    let base = r.bottom() - 36.0 * k;
    spaced(p, pos2(x - 3.0, base - 30.0 * k), Align2::LEFT_BOTTOM, t!("menu.fly_2"), menu::bold(112.0 * k.max(0.6)), menu::mix(Color32::WHITE, ACCENT, lit), 5.0 * k);
    p.text(pos2(x, base - 8.0 * k), Align2::LEFT_CENTER, t!("menu.pick_a_place_on_the"), menu::medium(16.5), Color32::from_rgb(220, 226, 234));
    // its key or button, bottom right, on a key cap
    let hc = Color32::from_white_alpha(190);
    match Hint::of("Enter", "A") {
        Hint::Text(h) => {
            let g = p.layout_no_wrap(h.to_string(), FontId::monospace(11.5), hc);
            let cap = Rect::from_min_size(pos2(r.right() - 28.0 * k - g.size().x - 14.0, base - 8.0 * k - 11.0), vec2(g.size().x + 14.0, 22.0));
            p.rect_filled(cap, menu::R_WIDGET, Color32::from_white_alpha(18));
            p.rect_stroke(cap, menu::R_WIDGET, Stroke::new(1.0, Color32::from_white_alpha(46)), egui::StrokeKind::Inside);
            p.galley(cap.center() - g.size() * 0.5, g, hc);
        }
        Hint::Pad(spec) => {
            let h = 22.0;
            let w = padui::spec_width(p, spec, h);
            padui::paint_spec(p, r.right() - 26.0 * k - w, base - 8.0 * k, spec, h, hc);
        }
        Hint::None => {}
    }
}

/// Text with extra space between the letters.
fn spaced(p: &Painter, at: Pos2, align: Align2, text: &str, font: FontId, col: Color32, spacing: f32) {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: font, color: col, extra_letter_spacing: spacing, ..Default::default() });
    let g = p.layout_job(job);
    // (the spacing trails the last letter too: leave it out of the centring)
    let size = g.size() - vec2(spacing, 0.0);
    let r = align.anchor_size(at, size);
    p.galley(r.min, g, col);
}

/// A horizontal gradient over a rectangle, `left` to `right`.
fn hgradient(p: &Painter, r: Rect, left: Color32, right: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), left);
    mesh.colored_vertex(r.right_top(), right);
    mesh.colored_vertex(r.right_bottom(), right);
    mesh.colored_vertex(r.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

/// A vertical gradient over a rectangle.
fn gradient(p: &Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), top);
    mesh.colored_vertex(r.right_top(), top);
    mesh.colored_vertex(r.right_bottom(), bottom);
    mesh.colored_vertex(r.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

/// a cog turned by `turn` radians.
fn gear_icon_turned(p: &Painter, c: Pos2, s: f32, col: Color32, turn: f32) {
    let (r_out, r_in, teeth) = (s * 0.46, s * 0.36, 8);
    let mut pts = Vec::new();
    for i in 0..teeth {
        let a0 = turn + i as f32 / teeth as f32 * std::f32::consts::TAU;
        let step = std::f32::consts::TAU / teeth as f32;
        // root, rise, top across, fall
        for (f, r) in [(0.0, r_in), (0.18, r_in), (0.28, r_out), (0.62, r_out), (0.72, r_in)] {
            let a = a0 + f * step;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    let st = Stroke::new((s * 0.03).clamp(1.6, 3.0), col);
    p.add(egui::Shape::closed_line(pts, st));
    p.circle_stroke(c, s * 0.15, st);
}

/// The power symbol: a ring open at the top, a bar through the gap.
fn power_icon(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let r = s * 0.45;
    let st = Stroke::new(1.6, col);
    let pts: Vec<Pos2> = (0..=20).map(|i| {
        let a = -std::f32::consts::FRAC_PI_2 + 0.65 + i as f32 / 20.0 * (std::f32::consts::TAU - 1.3);
        c + vec2(a.cos(), a.sin()) * r
    }).collect();
    p.line(pts, st);
    p.line_segment([c - vec2(0.0, r * 1.15), c - vec2(0.0, r * 0.2)], st);
}
