//! Gamepad button prompts in the Xbox style (green A, red B, blue X,
//! yellow Y, bumpers, triggers, sticks, d-pad, View and Menu), and the
//! switch between keyboard hints and gamepad ones: the menus and the HUD
//! show whichever device was last used, so a pad that is merely plugged in
//! does not hide the keys.
//!
//! Prompts are written as short specs: button names separated by spaces,
//! with `+` and `/` drawn as text between them, e.g. `"Y + View"`, `"LT / RT"`.

use egui::{pos2, vec2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, Ui};
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// A gamepad was the last input: hints show its buttons.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Mark the keyboard and mouse as the last input (their hints show).
pub fn use_keyboard() {
    ACTIVE.store(false, Ordering::Relaxed);
}

/// Mark the gamepad as the last input (its hints show).
pub fn use_pad() {
    ACTIVE.store(true, Ordering::Relaxed);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Btn {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    View,
    Menu,
    /// the sticks, moved
    LS,
    RS,
    /// the sticks, clicked in
    L3,
    R3,
    /// the whole d-pad, and one direction of it
    Dpad,
    Up,
    Down,
    Left,
    Right,
}

impl Btn {
    pub fn parse(s: &str) -> Option<Btn> {
        Some(match s {
            "A" => Btn::A,
            "B" => Btn::B,
            "X" => Btn::X,
            "Y" => Btn::Y,
            "LB" => Btn::LB,
            "RB" => Btn::RB,
            "LT" => Btn::LT,
            "RT" => Btn::RT,
            "View" => Btn::View,
            "Menu" => Btn::Menu,
            "LS" => Btn::LS,
            "RS" => Btn::RS,
            "L3" => Btn::L3,
            "R3" => Btn::R3,
            "Dpad" => Btn::Dpad,
            "Up" => Btn::Up,
            "Down" => Btn::Down,
            "Left" => Btn::Left,
            "Right" => Btn::Right,
            _ => return None,
        })
    }

    /// Width of the glyph at a height.
    pub fn width(self, h: f32) -> f32 {
        match self {
            Btn::LB | Btn::RB => h * 1.65,
            Btn::LT | Btn::RT => h * 1.35,
            _ => h,
        }
    }
}

const BODY: Color32 = Color32::from_rgb(46, 52, 62);
const RIM: Color32 = Color32::from_rgb(196, 202, 212);
const INK: Color32 = Color32::from_rgb(240, 242, 245);

/// Text drawn twice, a hair apart: the only font has no bold face.
fn bold(p: &Painter, at: Pos2, s: &str, size: f32, col: Color32) {
    let f = FontId::proportional(size);
    p.text(at, egui::Align2::CENTER_CENTER, s, f.clone(), col);
    p.text(at + vec2(0.45, 0.0), egui::Align2::CENTER_CENTER, s, f, col);
}

/// Draw one button glyph filling `r` (its height sets the size).
pub fn paint(p: &Painter, r: Rect, b: Btn) {
    let h = r.height();
    let c = r.center();
    let rad = h * 0.5 - 0.5;
    // the letters sit a touch low in the font's box
    let text_at = c + vec2(0.0, h * 0.02);
    let face = |col: Color32, ink: Color32, s: &str| {
        p.circle_filled(c + vec2(0.0, 1.0), rad, Color32::from_black_alpha(90));
        p.circle_filled(c, rad, col);
        p.circle_stroke(c, rad - 0.5, Stroke::new(1.0, Color32::from_white_alpha(60)));
        bold(p, text_at, s, h * 0.6, ink);
    };
    match b {
        Btn::A => face(Color32::from_rgb(84, 168, 58), Color32::WHITE, "A"),
        Btn::B => face(Color32::from_rgb(212, 52, 48), Color32::WHITE, "B"),
        Btn::X => face(Color32::from_rgb(38, 112, 214), Color32::WHITE, "X"),
        Btn::Y => face(Color32::from_rgb(236, 182, 24), Color32::from_rgb(40, 30, 0), "Y"),
        Btn::LB | Btn::RB | Btn::LT | Btn::RT => {
            let big = (h * 0.5) as u8;
            let small = (h * 0.16) as u8;
            // Bumpers sweep up to the outer corner; triggers are rounded over the top.
            let cr = match b {
                Btn::LB => CornerRadius { nw: big, ne: small, sw: small, se: small },
                Btn::RB => CornerRadius { nw: small, ne: big, sw: small, se: small },
                _ => CornerRadius { nw: big, ne: big, sw: small, se: small },
            };
            let body = if matches!(b, Btn::LB | Btn::RB) { r.shrink2(vec2(0.0, h * 0.06)) } else { r };
            p.rect_filled(body.translate(vec2(0.0, 1.0)), cr, Color32::from_black_alpha(90));
            p.rect_filled(body, cr, BODY);
            p.rect_stroke(body, cr, Stroke::new(1.0, RIM), egui::StrokeKind::Inside);
            let s = match b {
                Btn::LB => "LB",
                Btn::RB => "RB",
                Btn::LT => "LT",
                _ => "RT",
            };
            p.text(body.center() + vec2(0.0, h * 0.03), egui::Align2::CENTER_CENTER, s, FontId::proportional(h * 0.54), INK);
        }
        Btn::View | Btn::Menu => {
            p.circle_filled(c + vec2(0.0, 1.0), rad, Color32::from_black_alpha(90));
            p.circle_filled(c, rad, BODY);
            p.circle_stroke(c, rad - 0.5, Stroke::new(1.0, RIM));
            let u = h * 0.11;
            let st = Stroke::new((h * 0.07).max(1.0), INK);
            if b == Btn::Menu {
                for k in -1..=1 {
                    let y = c.y + k as f32 * u * 1.5;
                    p.line_segment([pos2(c.x - u * 2.2, y), pos2(c.x + u * 2.2, y)], st);
                }
            } else {
                // two windows, one over the other
                let a = Rect::from_min_size(c + vec2(-u * 2.3, -u * 1.9), vec2(u * 3.0, u * 2.4));
                let b2 = a.translate(vec2(u * 1.5, u * 1.4));
                p.rect_stroke(a, 1.0, st, egui::StrokeKind::Middle);
                p.rect_filled(b2, 1.0, BODY);
                p.rect_stroke(b2, 1.0, st, egui::StrokeKind::Middle);
            }
        }
        Btn::LS | Btn::RS | Btn::L3 | Btn::R3 => {
            let click = matches!(b, Btn::L3 | Btn::R3);
            p.circle_filled(c + vec2(0.0, 1.0), rad, Color32::from_black_alpha(90));
            p.circle_filled(c, rad, BODY);
            p.circle_stroke(c, rad - 0.5, Stroke::new(if click { 2.0 } else { 1.0 }, RIM));
            p.circle_stroke(c, rad * 0.62, Stroke::new(1.0, Color32::from_white_alpha(70)));
            let letter = if matches!(b, Btn::LS | Btn::L3) { "L" } else { "R" };
            if click {
                p.text(text_at, egui::Align2::CENTER_CENTER, format!("{letter}S"), FontId::proportional(h * 0.42), INK);
            } else {
                bold(p, text_at, letter, h * 0.5, INK);
                // arrow ticks all round: the stick is moved
                let t = h * 0.09;
                for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                    let d = vec2(dx, dy);
                    let tip = c + d * (rad - 1.2);
                    let back = tip - d * t * 1.3;
                    let side = vec2(-dy, dx) * t;
                    p.add(egui::Shape::convex_polygon(vec![tip, back + side, back - side], INK, Stroke::NONE));
                }
            }
        }
        Btn::Dpad | Btn::Up | Btn::Down | Btn::Left | Btn::Right => {
            let arm = h * 0.4;
            let len = h * 0.5 - 0.5;
            let v = Rect::from_center_size(c, vec2(arm, len * 2.0));
            let hz = Rect::from_center_size(c, vec2(len * 2.0, arm));
            for rr in [v, hz] {
                p.rect_filled(rr.translate(vec2(0.0, 1.0)), 2.0, Color32::from_black_alpha(90));
            }
            for rr in [v, hz] {
                p.rect_filled(rr, 2.0, BODY);
            }
            // the direction meant is lit; the whole pad has every arm lit
            let lit = |d: Btn| b == Btn::Dpad || b == d;
            // (each arm from the middle out)
            let arms = [
                (Btn::Up, Rect::from_min_max(pos2(v.left(), v.top()), pos2(v.right(), c.y))),
                (Btn::Down, Rect::from_min_max(pos2(v.left(), c.y), pos2(v.right(), v.bottom()))),
                (Btn::Left, Rect::from_min_max(pos2(hz.left(), hz.top()), pos2(c.x, hz.bottom()))),
                (Btn::Right, Rect::from_min_max(pos2(c.x, hz.top()), pos2(hz.right(), hz.bottom()))),
            ];
            for (d, rr) in arms {
                if lit(d) {
                    p.rect_filled(rr.shrink(1.0), 1.5, if b == Btn::Dpad { Color32::from_rgb(120, 128, 140) } else { INK });
                }
            }
            for rr in [v, hz] {
                p.rect_stroke(rr, 2.0, Stroke::new(1.0, RIM), egui::StrokeKind::Inside);
            }
        }
    }
}

enum Tok<'a> {
    Btn(Btn),
    Text(&'a str),
}

fn tokens(spec: &str) -> impl Iterator<Item = Tok<'_>> {
    spec.split_whitespace().map(|t| Btn::parse(t).map(Tok::Btn).unwrap_or(Tok::Text(t)))
}

const GAP: f32 = 3.0;

fn text_w(p: &Painter, s: &str, size: f32) -> f32 {
    p.layout_no_wrap(s.to_string(), FontId::proportional(size), Color32::WHITE).size().x
}

/// Width of a whole spec at a glyph height.
pub fn spec_width(p: &Painter, spec: &str, h: f32) -> f32 {
    let mut w = 0.0;
    for (k, t) in tokens(spec).enumerate() {
        if k > 0 {
            w += GAP;
        }
        w += match t {
            Tok::Btn(b) => b.width(h),
            Tok::Text(s) => text_w(p, s, h * 0.7),
        };
    }
    w
}

/// Paint a spec with its left edge at `x`, centred on `y`.
pub fn paint_spec(p: &Painter, x: f32, y: f32, spec: &str, h: f32, text: Color32) {
    let mut x = x;
    for (k, t) in tokens(spec).enumerate() {
        if k > 0 {
            x += GAP;
        }
        match t {
            Tok::Btn(b) => {
                let w = b.width(h);
                paint(p, Rect::from_min_size(pos2(x, y - h * 0.5), vec2(w, h)), b);
                x += w;
            }
            Tok::Text(s) => {
                let g = p.layout_no_wrap(s.to_string(), FontId::proportional(h * 0.7), text);
                let w = g.size().x;
                p.galley(pos2(x, y - g.size().y * 0.5), g, text);
                x += w;
            }
        }
    }
}

/// A spec as a widget, `h` points tall.
pub fn glyphs(ui: &mut Ui, spec: &str, h: f32) -> egui::Response {
    let w = spec_width(ui.painter(), spec, h);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
    paint_spec(ui.painter(), r.left(), r.center().y, spec, h, Color32::from_white_alpha(200));
    resp
}

/// A row of prompts: each spec's buttons, then what they do.
pub fn prompts(ui: &mut Ui, items: &[(&str, &str)], h: f32, size: f32, col: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (k, (spec, what)) in items.iter().enumerate() {
            if k > 0 {
                ui.add_space(10.0);
            }
            glyphs(ui, spec, h);
            ui.label(egui::RichText::new(*what).size(size).color(col));
        }
    });
}

/// A hint beside a menu button: a key, gamepad buttons, plain words, or none.
#[derive(Copy, Clone)]
pub enum Hint<'a> {
    None,
    Text(&'a str),
    Pad(&'a str),
}

impl<'a> Hint<'a> {
    /// the pad's buttons while a pad is in use, else nothing.
    pub fn of(_key: &'a str, pad: &'a str) -> Self {
        if active() && !pad.is_empty() { Hint::Pad(pad) } else { Hint::None }
    }
}

/// A clear ring round the widget the gamepad has focused, over everything.
pub fn focus_ring(ctx: &egui::Context, col: Color32) {
    if !active() {
        return;
    }
    let Some(id) = ctx.memory(|m| m.focused()) else { return };
    let Some(r) = ctx.read_response(id) else { return };
    let rect = r.interact_rect;
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("pad_focus")));
    p.rect_stroke(rect.expand(2.0), crate::menu::R_WIDGET, Stroke::new(2.0, col), egui::StrokeKind::Outside);
}

/// The scroll areas and the gamepad. The right stick's scrolling: how far
/// this frame asks for (points, + moves the content down, showing what is
/// above), and which area takes it: the one holding the focused widget, else
/// the largest in view, as seen on the previous pass (an area only knows
/// what it holds once its contents are laid out). And the widget each area
/// last brought into view, to follow the focus as the d-pad moves it.
struct Scroll {
    amount: f32,
    target: Option<egui::Id>,
    /// this pass's areas: (id, visible area, holds the focus)
    seen: Vec<(egui::Id, f32, bool)>,
    /// (area, the focused widget it last scrolled to)
    followed: Vec<(egui::Id, egui::Id)>,
}

thread_local! {
    static SCROLL: std::cell::RefCell<Scroll> = const { std::cell::RefCell::new(Scroll { amount: 0.0, target: None, seen: Vec::new(), followed: Vec::new() }) };
}

/// The right stick's scroll for this frame (points; 0 when it is centred).
pub fn set_scroll(points: f32) {
    SCROLL.with(|s| s.borrow_mut().amount = points);
}

/// At the start of a UI pass: settle which scroll area the stick works.
pub fn begin_pass() {
    SCROLL.with(|s| {
        let s = &mut *s.borrow_mut();
        s.target = s.seen.iter().find(|a| a.2).or_else(|| s.seen.iter().max_by(|a, b| a.1.total_cmp(&b.1))).map(|a| a.0);
        s.seen.clear();
    });
}

/// At the end of a scroll area's contents: scroll a widget the gamepad (or
/// the keyboard) has moved the focus to into view, and let the right stick
/// scroll the area (pages of text have nothing for the d-pad to move to).
///
/// The move is noticed by the focused widget changing, not by
/// `Response::gained_focus`: egui moves the focus at the end of a pass, so
/// by the next one that already reads false.
pub fn follow_focus(ui: &Ui) {
    let ctx = ui.ctx();
    let focus = ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id));
    let held = focus.filter(|r| r.layer_id == ui.layer_id() && ui.min_rect().contains_rect(r.rect));
    let area = ui.scope_id();
    let shown = ui.clip_rect().intersect(ui.min_rect());
    let vis = if shown.is_positive() { shown.area() } else { 0.0 };
    SCROLL.with(|s| {
        let s = &mut *s.borrow_mut();
        let last = s.followed.iter().position(|f| f.0 == area);
        match (&held, last) {
            (Some(r), Some(k)) if s.followed[k].1 == r.id => {}
            (Some(r), _) => {
                ui.scroll_to_rect(r.rect.expand(8.0), None);
                match last {
                    Some(k) => s.followed[k].1 = r.id,
                    None => s.followed.push((area, r.id)),
                }
            }
            // the focus left: coming back to the same widget scrolls to it again
            (None, Some(k)) => {
                s.followed.swap_remove(k);
            }
            (None, None) => {}
        }
        if !active() {
            return;
        }
        s.seen.push((area, vis, held.is_some()));
        if s.target == Some(area) && s.amount != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, s.amount), egui::style::ScrollAnimation::none());
            s.amount = 0.0;
            ctx.request_repaint();
        }
    });
}
