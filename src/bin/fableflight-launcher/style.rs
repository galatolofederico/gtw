//! The launcher's look, the game's own (src/menu.rs): the Barlow faces, the
//! dark panels and the aviation-yellow accent, with a few widgets drawn to match.
use egui::{Color32, CornerRadius, FontFamily, FontId, Rect, RichText, Sense, Stroke, Vec2, pos2, vec2};

pub const ACCENT: Color32 = Color32::from_rgb(245, 197, 24);
pub const ACCENT_HI: Color32 = Color32::from_rgb(255, 214, 72);
pub const ON_ACCENT: Color32 = Color32::from_rgb(26, 20, 2);
pub const TEXT: Color32 = Color32::from_rgb(232, 236, 242);
pub const DIM: Color32 = Color32::from_rgb(146, 156, 171);
pub const FAINT: Color32 = Color32::from_rgb(98, 108, 123);
pub const RAISED: Color32 = Color32::from_rgb(28, 34, 44);
pub const RAISED_HI: Color32 = Color32::from_rgb(42, 50, 63);
pub const CHOSEN: Color32 = Color32::from_rgb(66, 78, 98);
pub const LINE: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 30);
pub const LINK: Color32 = Color32::from_rgb(132, 184, 255);
pub const OK: Color32 = Color32::from_rgb(112, 214, 138);
pub const WARN: Color32 = Color32::from_rgb(255, 196, 92);
pub const BAD: Color32 = Color32::from_rgb(255, 122, 92);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(9, 12, 17, 214);
pub const DISCORD: Color32 = Color32::from_rgb(88, 101, 242);
pub const DISCORD_HI: Color32 = Color32::from_rgb(110, 122, 255);
pub const R: u8 = 4;

const BOLD: &str = "bold";
const MEDIUM: &str = "medium";
const SEMIBOLD: &str = "semibold";

pub fn fonts(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::empty();
    let add = |f: &mut egui::FontDefinitions, name: &str, data: &'static [u8]| {
        f.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(data)));
    };
    add(&mut f, "regular", include_bytes!("../../../assets/fonts/Barlow-Regular.ttf"));
    add(&mut f, MEDIUM, include_bytes!("../../../assets/fonts/Barlow-Medium.ttf"));
    add(&mut f, SEMIBOLD, include_bytes!("../../../assets/fonts/Barlow-SemiBold.ttf"));
    add(&mut f, BOLD, include_bytes!("../../../assets/fonts/BarlowCondensed-Bold.ttf"));
    add(&mut f, "mono", include_bytes!("../../../assets/fonts/IBMPlexMono-Medium.ttf"));
    f.families.insert(FontFamily::Proportional, vec!["regular".into(), "mono".into()]);
    f.families.insert(FontFamily::Monospace, vec!["mono".into(), "regular".into()]);
    for n in [BOLD, MEDIUM, SEMIBOLD] {
        f.families.insert(FontFamily::Name(n.into()), vec![n.into(), "regular".into()]);
    }
    ctx.set_fonts(f);
}

pub fn visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = Color32::from_rgb(15, 19, 26);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = LINK;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.hovered.bg_fill = RAISED_HI;
    v.widgets.hovered.weak_bg_fill = RAISED_HI;
    v.widgets.active.bg_fill = CHOSEN;
    v.widgets.active.weak_bg_fill = CHOSEN;
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.slider_width = 220.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::new(15.0, FontFamily::Name(MEDIUM.into())));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(13.0));
    });
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn ease(ctx: &egui::Context, id: egui::Id, on: bool) -> f32 {
    ctx.animate_bool_with_time_and_easing(id.with("ease"), on, 0.14, egui::emath::easing::cubic_out)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// A dark glass card.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(R + 2)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card's heading in display capitals.
pub fn heading(ui: &mut egui::Ui, s: &str) {
    ui.label(RichText::new(s.to_uppercase()).font(bold(16.0)).color(Color32::WHITE).extra_letter_spacing(1.0));
}

pub fn page_title(ui: &mut egui::Ui, title: &str, blurb: &str) {
    ui.label(RichText::new(title).font(semibold(30.0)).color(Color32::WHITE));
    if !blurb.is_empty() {
        ui.label(RichText::new(blurb).color(DIM));
    }
    ui.add_space(6.0);
}

/// Small grey text.
pub fn note(ui: &mut egui::Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).size(13.0).color(DIM));
}

/// A label with a value beside it, for the cards' facts.
pub fn fact(ui: &mut egui::Ui, k: &str, v: impl Into<RichText>) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(90.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(90.0);
            ui.label(RichText::new(k).size(13.0).color(DIM));
        });
        ui.label(v.into());
    });
}

/// A pill-shaped tag.
pub fn chip(ui: &mut egui::Ui, text: &str, col: Color32) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.to_uppercase(), bold(12.5), col);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, col.gamma_multiply(0.16));
    ui.painter().rect_stroke(rect, 10.0, Stroke::new(1.0, col.gamma_multiply(0.5)), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - g.size() * 0.5, g, col);
    resp
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// the one thing to press: yellow
    Primary,
    /// everything else
    Plain,
    /// Discord's blurple
    Discord,
    /// stops something: red outline
    Danger,
}

/// A button, with an icon before its text if given.
pub fn button(ui: &mut egui::Ui, text: &str, icon: Option<&egui::TextureHandle>, kind: Kind, size: Vec2) -> egui::Response {
    let font = medium(if size.y >= 44.0 { 17.0 } else { 15.0 });
    let enabled = ui.is_enabled();
    let (fg, bg, bg_hi) = match kind {
        Kind::Primary => (ON_ACCENT, ACCENT, ACCENT_HI),
        Kind::Plain => (TEXT, RAISED, RAISED_HI),
        Kind::Discord => (Color32::WHITE, DISCORD, DISCORD_HI),
        Kind::Danger => (BAD, Color32::from_rgba_premultiplied(40, 12, 8, 120), Color32::from_rgba_premultiplied(70, 20, 12, 160)),
    };
    let g = ui.painter().layout_no_wrap(text.to_string(), font, if enabled { fg } else { FAINT });
    let icon_w = if icon.is_some() { 18.0 + 8.0 } else { 0.0 };
    let w = size.x.max(g.size().x + icon_w + 28.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, size.y), Sense::click());
    let lit = ease(ui.ctx(), resp.id, resp.hovered() && enabled);
    let p = ui.painter();
    let fill = if enabled { mix(bg, bg_hi, lit) } else { RAISED.gamma_multiply(0.6) };
    let rect_drawn = if resp.is_pointer_button_down_on() && enabled { rect.shrink(1.0) } else { rect };
    p.rect_filled(rect_drawn, R, fill);
    if kind == Kind::Danger {
        p.rect_stroke(rect_drawn, R, Stroke::new(1.0, BAD.gamma_multiply(0.6)), egui::StrokeKind::Inside);
    }
    let total = g.size().x + icon_w;
    let x0 = rect.center().x - total * 0.5;
    if let Some(t) = icon {
        let ir = Rect::from_center_size(pos2(x0 + 9.0, rect.center().y), vec2(18.0, 18.0));
        let tint = if kind == Kind::Plain { TEXT } else { fg };
        p.image(t.id(), ir, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), if enabled { tint } else { FAINT });
    }
    p.galley(pos2(x0 + icon_w, rect.center().y - g.size().y * 0.5), g, fg);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A segmented choice: one of `options`, the chosen one lit.
pub fn segments<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    egui::Frame::new().fill(Color32::from_rgba_premultiplied(0, 0, 0, 80)).stroke(Stroke::new(1.0, LINE)).corner_radius(R + 1).inner_margin(3).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (v, label) in options {
                let on = *current == *v;
                let g = ui.painter().layout_no_wrap(label.to_string(), medium(14.5), TEXT);
                let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 26.0), Sense::click());
                let lit = ease(ui.ctx(), resp.id, resp.hovered() && !on);
                let p = ui.painter();
                if on {
                    p.rect_filled(rect, R, CHOSEN);
                } else if lit > 0.0 {
                    p.rect_filled(rect, R, Color32::from_white_alpha((12.0 * lit) as u8));
                }
                let col = if !ui.is_enabled() { FAINT } else if on { Color32::WHITE } else { mix(DIM, TEXT, lit) };
                p.galley(rect.center() - g.size() * 0.5, g, col);
                if resp.clicked() && !on {
                    *current = *v;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// An on/off switch.
pub fn switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (w, h) = (36.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time_and_easing(resp.id.with("knob"), *on, 0.16, egui::emath::easing::cubic_out);
    let lit = ease(ui.ctx(), resp.id, resp.hovered());
    let track = Rect::from_center_size(rect.center(), vec2(w, h));
    let p = ui.painter();
    p.rect_filled(track, h * 0.5, mix(mix(RAISED, RAISED_HI, lit), mix(ACCENT, ACCENT_HI, lit), t));
    let knob = pos2(track.left() + h * 0.5 + (w - h) * t, track.center().y);
    p.circle_filled(knob, h * 0.5 - 3.0, mix(DIM, ON_ACCENT, t));
    resp.clicked()
}

/// One setting: its name and hint on the left, the control on the right.
pub fn row<R>(ui: &mut egui::Ui, name: &str, hint: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(320.0, 36.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_width(320.0);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(name).font(medium(15.0)).color(TEXT));
            if !hint.is_empty() {
                ui.label(RichText::new(hint).size(12.5).color(FAINT));
            }
        });
        out = Some(add(ui));
    });
    ui.add_space(4.0);
    out.unwrap()
}

/// A progress bar the launcher's way: a thin track, the yellow fill with a
/// sheen running along it while something is under way.
pub fn progress_bar(ui: &mut egui::Ui, rect: Rect, frac: Option<f32>, busy: bool, col: Color32) {
    let p = ui.painter();
    let r = CornerRadius::same((rect.height() * 0.5) as u8);
    p.rect_filled(rect, r, Color32::from_white_alpha(22));
    let t = ui.input(|i| i.time) as f32;
    match frac {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            if f > 0.0 {
                let fill = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(rect.height()), rect.height()));
                p.rect_filled(fill, r, col);
                if busy {
                    // the sheen
                    let x = fill.left() + ((t * 0.6).fract() * (fill.width() + 160.0)) - 80.0;
                    let sheen = Rect::from_center_size(pos2(x, fill.center().y), vec2(80.0, fill.height())).intersect(fill);
                    if sheen.width() > 0.0 {
                        p.rect_filled(sheen, r, Color32::from_white_alpha(46));
                    }
                }
            }
        }
        None if busy => {
            // waiting on something of unknown length: a block sweeping across
            let w = rect.width() * 0.25;
            let x = rect.left() - w + (t * 0.5).fract() * (rect.width() + w);
            let block = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height())).intersect(rect);
            p.rect_filled(block, r, col);
        }
        None => {}
    }
}

/// Seconds as "12.4 s" or "1 min 05 s".
pub fn secs(s: f64) -> String {
    if s < 60.0 { format!("{s:.1} s") } else { format!("{} min {:02} s", (s / 60.0) as u64, (s % 60.0) as u64) }
}

pub fn bytes(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.2} GB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", n as f64 / 1024.0)
    }
}
