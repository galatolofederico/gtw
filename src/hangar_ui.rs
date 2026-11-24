//! The globe's aircraft card, and the aircraft screen it opens (also from
//! the main menu): every aircraft you can fly in a list with the one picked
//! out large, and FlightGear's collection to download more from (see
//! hangar.rs), each with the license its files state.
use crate::aircraft::{self, Spec};
use crate::hangar::{self, Install, LicenseFrom, State, Status};
use crate::i18n::{t, tr_en};
use crate::menu::{self, ACCENT, DIM, TEXT};
use egui::{Align2, Color32, RichText, Stroke};

/// Which page of the hangar is showing.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tab {
    Mine,
    Download,
}

const OK: Color32 = Color32::from_rgb(120, 214, 140);
const BAD: Color32 = Color32::from_rgb(255, 128, 96);

impl crate::Game {
    /// An aircraft's picture (shared by types drawn with the same body).
    pub(crate) fn still(&mut self, ctx: &egui::Context, spec: &Spec) -> egui::TextureHandle {
        // (In the paint scheme picked for it, for a type that has several.)
        if let Some(l) = crate::aircraft::liveries(spec).get(self.menu.livery(spec)) {
            return self.picture(ctx, l.still);
        }
        self.picture(ctx, spec.still)
    }

    /// A picture embedded in the game (a PNG), as a texture, made once.
    pub(crate) fn picture(&mut self, ctx: &egui::Context, png: &'static [u8]) -> egui::TextureHandle {
        let key = png.as_ptr() as usize;
        self.stills
            .entry(key)
            .or_insert_with(|| {
                let img = image::load_from_memory(png).expect("embedded picture").to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(format!("picture {key}"), egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR)
            })
            .clone()
    }

    /// Top left of the globe: the chosen aircraft, arrows to the others in
    /// its title bar and its picture (click: the aircraft screen). Returns
    /// its rectangle.
    pub(crate) fn aircraft_window(&mut self, ctx: &egui::Context, w: f32, at: egui::Pos2) -> egui::Rect {
        let list = aircraft::list();
        let n = list.len();
        let mut choose: Option<usize> = None;
        let mut open: Option<Tab> = None;
        let k = self.menu.aircraft.min(n - 1);
        let spec = list[k];
        let tex = self.still(ctx, spec);
        let air = egui::Area::new(egui::Id::unique("menu_air")).fixed_pos(at).show(ctx, |ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = egui::CornerRadius { nw: menu::R_PANEL, ne: menu::R_PANEL, sw: 0, se: 0 };
            let bottom = egui::CornerRadius { nw: 0, ne: 0, sw: menu::R_PANEL, se: menu::R_PANEL };
            egui::Frame::new().fill(menu::PANEL).corner_radius(top).inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 6 }).show(ui, |ui| {
                ui.set_width(w - 20.0);
                ui.horizontal(|ui| {
                    let arrow = |ui: &mut egui::Ui, turn: f32| {
                        let r = ui.add(egui::Button::new("").min_size(egui::vec2(34.0, 34.0)).fill(Color32::from_white_alpha(8)).corner_radius(menu::R_WIDGET));
                        menu::chevron(ui.painter(), r.rect.center(), 6.0, turn, if r.hovered() || r.has_focus() { Color32::WHITE } else { DIM });
                        r
                    };
                    if arrow(ui, 2.0).on_hover_text(t!("hangar.previous_aircraft")).clicked() {
                        choose = Some((k + n - 1) % n);
                    }
                    let name_w = ui.available_width() - 34.0 - ui.spacing().item_spacing.x;
                    ui.allocate_ui_with_layout(egui::vec2(name_w, 34.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.label(menu::section_text(&t!("hangar.aircraft", k + 1, n)).color(DIM));
                        ui.label(RichText::new(spec.name).font(menu::item_title()).color(Color32::WHITE));
                    });
                    if arrow(ui, 0.0).on_hover_text(t!("hangar.next_aircraft")).clicked() {
                        choose = Some((k + 1) % n);
                    }
                });
            });
            // a little shorter than the picture, whose edges are empty.
            let ph = w * 0.46;
            let (pic, resp) = ui.allocate_exact_size(egui::vec2(w, ph), egui::Sense::click());
            ui.painter().rect_filled(pic, 0.0, menu::PANEL);
            let iw = (ph * 680.0 / 392.0 * 1.08).min(w);
            let full = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().with_clip_rect(pic).image(tex.id(), egui::Rect::from_center_size(pic.center(), egui::vec2(iw, iw * 392.0 / 680.0)), full, Color32::WHITE);
            if spec.origin.is_some() {
                badge(ui.painter(), pic.left_top() + egui::vec2(12.0, 6.0), if spec.ext.is_some() { "FlightGear" } else { "JSBSim" });
            }
            if resp.hovered() {
                ui.painter().rect_filled(pic, 0.0, Color32::from_black_alpha(90));
                ui.painter().text(pic.center(), Align2::CENTER_CENTER, t!("hangar.see_all_aircraft"), egui::FontId::proportional(16.0), Color32::WHITE);
            }
            let resp = resp.on_hover_text(tr_en(spec.blurb));
            if resp.clicked() {
                open = Some(Tab::Mine);
            }
            egui::Frame::new().fill(menu::PANEL).corner_radius(bottom).inner_margin(egui::Margin { left: 10, right: 10, top: 4, bottom: 10 }).show(ui, |ui| {
                ui.set_width(w - 20.0);
                ui.columns(2, |c| {
                    if menu::wide_button(&mut c[0], t!("hangar.all_aircraft"), crate::padui::Hint::None, false, true).clicked() {
                        open = Some(Tab::Mine);
                    }
                    if menu::wide_button(&mut c[1], t!("hangar.get_more"), crate::padui::Hint::None, false, true).clicked() {
                        open = Some(Tab::Download);
                    }
                });
            });
        });
        let r = air.response.rect;
        ctx.layer_painter(air.response.layer_id).rect_stroke(r, menu::R_PANEL, Stroke::new(1.0, Color32::from_white_alpha(22)), egui::StrokeKind::Inside);
        if let Some(k) = choose {
            self.menu.aircraft = k;
        }
        if let Some(t) = open {
            self.open_aircraft(t, menu::Screen::Fly);
        }
        r
    }

    /// Opens the aircraft screen on a page, to go back to `back` when closed.
    pub(crate) fn open_aircraft(&mut self, tab: Tab, back: menu::Screen) {
        self.menu.air_tab = tab;
        self.menu.air_back = back;
        self.menu.air_view = self.menu.aircraft;
        self.menu.screen = menu::Screen::Aircraft;
        self.pad_home = true;
        if tab == Tab::Download {
            hangar::open();
        }
    }

    /// Leaves the aircraft screen (Back, Esc, the gamepad's B): from the
    /// download page to the list first.
    pub(crate) fn aircraft_back(&mut self) {
        if self.menu.air_tab == Tab::Download {
            self.menu.air_tab = Tab::Mine;
        } else {
            self.menu.screen = self.menu.air_back;
        }
        self.pad_home = true;
    }

    /// The aircraft screen, over the whole window: every aircraft in a list
    /// on the left (and more to download at its foot), the one picked out
    /// large on the right with its numbers and a Select button. Selecting
    /// sets `menu.aircraft`, the same choice the globe's aircraft card shows.
    pub(crate) fn aircraft_screen(&mut self, ctx: &egui::Context) {
        use egui::{pos2, vec2, Layout, Rect, Sense};
        let screen = ctx.content_rect();
        let list = aircraft::list();
        let n = list.len();
        self.menu.air_view = self.menu.air_view.min(n - 1);
        let texs: Vec<_> = list.iter().map(|s| self.still(ctx, s)).collect();
        let mut back = false;
        let mut select: Option<usize> = None;
        let mut tab = self.menu.air_tab;
        egui::Area::new(egui::Id::unique("aircraft_screen")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.interact(screen, ui.scope_id().with("backdrop"), Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(screen, 0.0, menu::BG);
            let side_w = if screen.width() < 1100.0 { 280.0 } else { 330.0 };
            let side = Rect::from_min_size(screen.min, vec2(side_w, screen.height()));
            p.rect_filled(side, 0.0, Color32::from_black_alpha(110));
            p.vline(side.right(), side.y_range(), Stroke::new(1.0, menu::LINE_HI));

            // The sidebar: title, every aircraft, more to download, Back.
            let foot_h = if crate::padui::active() { 96.0 } else { 74.0 };
            let list_r = Rect::from_min_max(side.min + vec2(16.0, 28.0), pos2(side.right() - 16.0, side.bottom() - foot_h - 12.0));
            let mut s = ui.new_child(egui::UiBuilder::new().max_rect(list_r).layout(Layout::top_down(egui::Align::Min)));
            s.add_space(10.0);
            menu::screen_heading(&mut s, t!("hangar.aircraft_2"));
            s.add_space(20.0);
            egui::ScrollArea::vertical().id_salt("aircraft_list").auto_shrink([false, false]).max_height(s.available_height()).show(&mut s, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (k, spec) in list.iter().enumerate() {
                    let on = tab == Tab::Mine && k == self.menu.air_view;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, spec.name));
                    if on {
                        self.pad_home_focus(&resp);
                    }
                    let lit = menu::ease(ui.ctx(), resp.id, (resp.hovered() || resp.has_focus()) && !on);
                    let pick = ui.ctx().animate_bool_with_time(resp.id.with("on"), on, 0.18);
                    let p = ui.painter();
                    let a = (7.0 * lit + 14.0 * pick).min(16.0);
                    if a > 0.0 {
                        p.rect_filled(rect, menu::R_WIDGET, Color32::from_white_alpha(a as u8));
                    }
                    if pick > 0.0 {
                        p.rect_filled(Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, rect.height() * 0.5 * pick)), 1.0, ACCENT);
                    }
                    let pic = Rect::from_min_size(pos2(rect.left() + 12.0 + 3.0 * lit, rect.center().y - 23.0), vec2(80.0, 46.0));
                    egui::Image::new(&texs[k]).paint_at(ui, pic);
                    let x = rect.left() + 104.0;
                    let chosen = k == self.menu.aircraft;
                    let name_col = menu::mix(menu::mix(Color32::from_rgb(200, 207, 218), TEXT, lit), Color32::WHITE, pick);
                    let room = rect.right() - x - 16.0;
                    let g = one_line_font(p, spec.name, menu::label(), name_col, room);
                    let name_w = g.size().x;
                    p.galley(pos2(x, rect.center().y - 18.0), g, name_col);
                    p.galley(pos2(x, rect.center().y + 2.0), one_line(p, &kind_line(spec), 13.0, DIM, room + 10.0), DIM);
                    if chosen {
                        // the one flown: a small accent dot after its name
                        p.circle_filled(pos2((x + name_w + 9.0).min(rect.right() - 6.0), rect.center().y - 8.0), 3.5, ACCENT);
                    }
                    if resp.clicked() {
                        self.menu.air_view = k;
                        tab = Tab::Mine;
                    }
                    if resp.double_clicked() {
                        select = Some(k);
                    }
                }
                ui.add_space(4.0);
                // Last: more to download.
                let on = tab == Tab::Download;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, t!("hangar.download_more")));
                if on {
                    self.pad_home_focus(&resp);
                }
                let lit = menu::ease(ui.ctx(), resp.id, resp.hovered() || resp.has_focus());
                let p = ui.painter();
                p.rect_filled(rect, menu::R_WIDGET, Color32::from_white_alpha(if on { 16 } else { (4.0 + 6.0 * lit) as u8 }));
                if on {
                    p.rect_filled(Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, rect.height() * 0.5)), 1.0, ACCENT);
                } else {
                    p.rect_stroke(rect, menu::R_WIDGET, Stroke::new(1.0, menu::mix(menu::LINE_HI, Color32::from_white_alpha(60), lit)), egui::StrokeKind::Inside);
                }
                let c = pos2(rect.left() + 52.0, rect.center().y);
                let col = if on { ACCENT } else { menu::mix(DIM, ACCENT, lit) };
                p.line_segment([c - vec2(9.0, 0.0), c + vec2(9.0, 0.0)], Stroke::new(2.0, col));
                p.line_segment([c - vec2(0.0, 9.0), c + vec2(0.0, 9.0)], Stroke::new(2.0, col));
                p.text(pos2(rect.left() + 104.0, rect.center().y - 9.0), Align2::LEFT_CENTER, t!("hangar.download_more"), menu::label(), if on { Color32::WHITE } else { menu::mix(TEXT, Color32::WHITE, lit) });
                p.text(pos2(rect.left() + 104.0, rect.center().y + 10.0), Align2::LEFT_CENTER, t!("hangar.flightgears_collection"), menu::caption(), DIM);
                if resp.clicked() && tab != Tab::Download {
                    tab = Tab::Download;
                    hangar::open();
                }
                crate::padui::follow_focus(ui);
            });
            let foot = Rect::from_min_max(pos2(side.left() + 16.0, side.bottom() - foot_h), pos2(side.right() - 16.0, side.bottom() - 24.0));
            let mut f = ui.new_child(egui::UiBuilder::new().max_rect(foot).layout(Layout::bottom_up(egui::Align::Min)));
            let back_to = if self.menu.air_back == menu::Screen::Fly { t!("hangar.back_to_the_globe") } else { t!("hangar.back") };
            if menu::wide_button(&mut f, back_to, crate::padui::Hint::of("Esc", "B"), false, true).clicked() {
                back = true;
            }
            if crate::padui::active() {
                f.add_space(6.0);
                crate::padui::prompts(&mut f, &[("LB / RB", tr_en("aircraft")), ("A", tr_en("press")), ("B", tr_en("back"))], 18.0, 12.0, DIM);
            }

            // The content: the aircraft picked out, or the download page.
            let area = Rect::from_min_max(pos2(side.right() + 1.0, screen.top()), screen.max);
            let col_w = (area.width() - 96.0).clamp(360.0, 1000.0);
            let left = area.left() + ((area.width() - col_w) * 0.5).max(24.0);
            let col = Rect::from_min_max(pos2(left, area.top() + 36.0), pos2(left + col_w, area.bottom() - 28.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(col).layout(Layout::top_down(egui::Align::Min)));
            match tab {
                Tab::Mine => {
                    let k = self.menu.air_view;
                    if let Some(go) = self.aircraft_detail(&mut c, list[k], &texs[k], k == self.menu.aircraft, col.height()) {
                        if go {
                            select = Some(k);
                        }
                    }
                }
                Tab::Download => {
                    c.label(RichText::new(t!("hangar.download_more_aircraft")).font(menu::page_title()).color(Color32::WHITE));
                    c.add_space(6.0);
                    if let Some(k) = self.downloads(&mut c) {
                        select = Some(k);
                    }
                }
            }
            ui.expand_to_include_rect(screen);
        });
        if tab != self.menu.air_tab {
            self.menu.air_tab = tab;
            self.pad_home = true;
        }
        if let Some(k) = select {
            self.menu.aircraft = k;
            self.menu.air_view = k;
            self.menu.air_tab = Tab::Mine;
            self.menu.screen = self.menu.air_back;
            self.pad_home = true;
        } else if back {
            self.aircraft_back();
        }
    }

    /// The aircraft screen's right side: the picture large, its name, a line
    /// on it, its numbers, and Select. Some(true): Select was pressed.
    fn aircraft_detail(&mut self, ui: &mut egui::Ui, spec: &Spec, tex: &egui::TextureHandle, chosen: bool, h: f32) -> Option<bool> {
        let w = ui.available_width();
        // The picture on a soft lit backdrop, no taller than about half the window.
        let ph = (w * 392.0 / 680.0).min((h * 0.5).max(220.0));
        let (stage, _) = ui.allocate_exact_size(egui::vec2(w, ph), egui::Sense::hover());
        {
            let p = ui.painter();
            let mut mesh = egui::Mesh::default();
            let (top, bot) = (Color32::from_rgb(28, 35, 47), Color32::from_rgb(10, 13, 18));
            mesh.colored_vertex(stage.left_top(), top);
            mesh.colored_vertex(stage.right_top(), top);
            mesh.colored_vertex(stage.right_bottom(), bot);
            mesh.colored_vertex(stage.left_bottom(), bot);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            p.add(mesh);
            // a pool of light and a faint floor line under it.
            let floor = stage.bottom() - ph * 0.12;
            let sp = p.with_clip_rect(stage);
            menu::glow(&sp, egui::pos2(stage.center().x, floor), egui::vec2(stage.width() * 0.42, ph * 0.2), Color32::from_white_alpha(22));
            p.hline(stage.x_range().shrink(stage.width() * 0.12), floor, Stroke::new(1.0, Color32::from_white_alpha(16)));
            p.rect_stroke(stage, menu::R_PANEL, Stroke::new(1.0, menu::LINE), egui::StrokeKind::Inside);
        }
        let iw = (ph * 680.0 / 392.0).min(w);
        let img = egui::Rect::from_center_size(stage.center(), egui::vec2(iw, iw * 392.0 / 680.0));
        egui::Image::new(tex).paint_at(ui, img);
        if spec.origin.is_some() {
            badge(ui.painter(), stage.left_top() + egui::vec2(14.0, 12.0), if spec.ext.is_some() { "FlightGear" } else { "JSBSim" });
        }
        if chosen {
            // the aircraft you will fly gets a mark in the corner.
            let g = ui.painter().layout_no_wrap(t!("hangar.in_use").to_string(), menu::bold(13.0), ACCENT);
            let r = egui::Rect::from_min_size(stage.right_top() + egui::vec2(-16.0 - g.size().x - 22.0, 14.0), g.size() + egui::vec2(22.0, 6.0));
            ui.painter().rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, ACCENT.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            ui.painter().circle_filled(egui::pos2(r.left() + 8.0, r.center().y), 3.0, ACCENT);
            ui.painter().galley(egui::pos2(r.left() + 15.0, r.top() + 3.0), g, ACCENT);
        }
        ui.add_space(18.0);
        let mut go = None;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((w - 260.0).max(200.0));
                ui.label(RichText::new(spec.name).font(menu::page_title()).color(Color32::WHITE));
                ui.label(RichText::new(origin_line(spec)).font(menu::body()).color(DIM));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.set_width(240.0);
                let b = menu::wide_button(ui, t!("hangar.select"), crate::padui::Hint::of("Enter", "A"), true, true);
                if b.clicked() {
                    go = Some(true);
                }
            });
        });
        ui.add_space(8.0);
        ui.add(egui::Label::new(RichText::new(tr_en(spec.blurb)).size(16.0).color(TEXT)).wrap());
        let liveries = crate::aircraft::liveries(spec);
        if !liveries.is_empty() {
            ui.add_space(14.0);
            ui.label(RichText::new(t!("hangar.livery")).font(menu::caption()).color(DIM));
            let mut changed = false;
            let current = self.menu.livery(spec);
            ui.horizontal_wrapped(|ui| {
                for (i, l) in liveries.iter().enumerate() {
                    let selected = current == i;
                    let button = egui::Button::new(RichText::new(tr_en(l.name)).color(if selected { Color32::WHITE } else { TEXT }))
                        .fill(if selected { Color32::from_rgb(48, 59, 78) } else { menu::SURFACE })
                        .stroke(Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { ACCENT } else { menu::LINE }))
                        .corner_radius(menu::R_WIDGET);
                    if ui.add(button).clicked() && !selected {
                        self.menu.liveries.insert(spec.name, i);
                        changed = true;
                    }
                    // A swatch: the main colour over the trim.
                    let srgb = |c: [f32; 3]| Color32::from(egui::Rgba::from_rgb(c[0], c[1], c[2]));
                    let (top, under) = (srgb(l.swatch[0]), srgb(l.swatch[1]));
                    let swatch = ui.allocate_exact_size(egui::vec2(35.0, 14.0), egui::Sense::hover()).0;
                    let split = swatch.top() + 10.0;
                    ui.painter().rect_filled(egui::Rect::from_min_max(swatch.min, egui::pos2(swatch.right(), split)), 2.0, top);
                    ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(swatch.left(), split), swatch.max), 1.0, under);
                }
            });
            // (Shown on the flown aircraft at once; another type takes it when it is picked.)
            if changed && std::ptr::eq(spec, self.spec) { self.apply_livery(); }
            ui.add_space(12.0);
        }
        ui.add_space(18.0);
        // The numbers, in a row of tiles.
        let cells = numbers(spec);
        let cols = if w > 760.0 { 3 } else { 2 };
        let gap = 10.0;
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        for row in cells.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (k, v) in row {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(cw, 64.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, menu::R_PANEL, menu::SURFACE);
                    p.rect_stroke(r, menu::R_PANEL, Stroke::new(1.0, menu::LINE), egui::StrokeKind::Inside);
                    p.text(r.left_top() + egui::vec2(16.0, 19.0), Align2::LEFT_CENTER, *k, menu::caption(), DIM);
                    p.text(r.left_top() + egui::vec2(16.0, 43.0), Align2::LEFT_CENTER, v, menu::item_title(), Color32::WHITE);
                }
            });
            ui.add_space(gap - ui.spacing().item_spacing.y);
        }
        if let Some(o) = spec.origin {
            ui.add_space(6.0);
            let by = if o.author.is_empty() { String::new() } else { t!("hangar.by", o.author) };
            ui.label(RichText::new(t!("hangar.license", o.license, by)).font(menu::caption()).color(DIM));
        }
        go
    }

    /// The JSBSim collection: a list on the left, the chosen one's details,
    /// license and download button on the right. Some(k): fly aircraft k.
    fn downloads(&mut self, ui: &mut egui::Ui) -> Option<usize> {
        let mut fly = None;
        let link = |ui: &mut egui::Ui, text: &str, url: &str| {
            ui.hyperlink_to(RichText::new(text).font(menu::caption()), url);
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new(t!("hangar.aircraft_from")).font(menu::caption()).color(DIM));
            link(ui, t!("hangar.flightgears_collection_fgaddon"), &hangar::repo_url());
            ui.label(RichText::new(t!("hangar.each_with_its_own_3d")).font(menu::caption()).color(DIM));
        });
        ui.add_space(8.0);
        let (status, entries) = {
            let c = hangar::catalogue();
            (c.status.clone(), c.entries.clone())
        };
        match status {
            Status::Idle | Status::Listing => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new(t!("hangar.reading_flightgears_aircraft_catalogue")).color(TEXT));
                });
                ui.ctx().request_repaint();
                return None;
            }
            Status::Failed(e) => {
                ui.label(RichText::new(t!("hangar.could_not_get_the_catalogue", e)).color(BAD));
                if ui.button(t!("hangar.try_again")).clicked() {
                    hangar::catalogue().status = Status::Idle;
                    hangar::open();
                }
                return None;
            }
            Status::Ready => {}
        }
        if hangar::reading() > 0 || entries.iter().any(|e| matches!(e.install, Install::Downloading(..) | Install::Unpacking)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let flyable = |e: &hangar::Entry| matches!(&e.state, State::Ready(i) if i.fit.is_ok());
        if self.menu.hangar_sel.as_ref().is_none_or(|s| !entries.iter().any(|e| &e.folder == s)) {
            self.menu.hangar_sel = entries.iter().find(|e| flyable(e)).or(entries.iter().find(|e| hangar::named_fdm(e) == Some(true))).or(entries.first()).map(|e| e.folder.clone());
        }
        let left_w = 300.0;
        let full = ui.available_rect_before_wrap();
        let h = full.height();
        let left = egui::Rect::from_min_size(full.min, egui::vec2(left_w, h));
        let right = egui::Rect::from_min_max(egui::pos2(left.right() + 14.0, full.top()), full.max);
        let column = |r: egui::Rect| egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min));
        ui.allocate_rect(full, egui::Sense::hover());
        {
            // The list.
            ui.scope_builder(column(left), |ui| {
                ui.set_width(left_w);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.menu.hangar_find).hint_text(t!("hangar.search")).desired_width(left_w - 120.0).margin(egui::vec2(8.0, 5.0)));
                    menu::toggle(ui, &mut self.menu.hangar_all, t!("hangar.show_all")).on_hover_text(t!("hangar.also_list_the_ones_that"));
                });
                let line = t!("hangar.aircraft_pick_one_to_see", entries.len());
                ui.label(RichText::new(line).font(menu::caption()).color(DIM));
                ui.add_space(4.0);
                let find = self.menu.hangar_find.to_lowercase();
                egui::ScrollArea::vertical().id_salt("hangar_list").auto_shrink([false, false]).max_height(ui.available_height()).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for e in &entries {
                        let name = match &e.state {
                            State::Ready(i) => i.name.clone(),
                            _ if !e.name.is_empty() => e.name.clone(),
                            _ => e.folder.clone(),
                        };
                        let can = flyable(e);
                        let failed = matches!(e.state, State::Failed(_)) || matches!(&e.state, State::Ready(i) if i.fit.is_err());
                        if failed && !self.menu.hangar_all && e.install != Install::Installed {
                            continue;
                        }
                        if !find.is_empty() && !name.to_lowercase().contains(&find) && !e.folder.to_lowercase().contains(&find) {
                            continue;
                        }
                        let sel = self.menu.hangar_sel.as_deref() == Some(e.folder.as_str());
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
                        let lit = menu::ease(ui.ctx(), resp.id, resp.hovered() || resp.has_focus());
                        let p = ui.painter();
                        // chosen: raised, with the accent mark at its left.
                        p.rect_filled(rect, menu::R_WIDGET, Color32::from_white_alpha(if sel { 16 } else { (4.0 + 8.0 * lit) as u8 }));
                        if sel {
                            p.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.left() + 1.5, rect.center().y), egui::vec2(3.0, rect.height() * 0.5)), 1.0, ACCENT);
                        }
                        let name_col = if can || matches!(e.state, State::Pending) { Color32::WHITE } else { DIM };
                        p.text(rect.left_center() + egui::vec2(10.0, -7.0), Align2::LEFT_CENTER, &name, menu::body(), name_col);
                        let sub = match &e.state {
                            State::Ready(i) => match &i.fit {
                                Ok(s) => format!("{} · {}", s.kind, i.license),
                                Err(_) => format!("{} · {}", t!("hangar.cant_be_flown_here"), i.license),
                            },
                            State::Pending => match hangar::named_fdm(e) {
                                Some(true) => format!("JSBSim · {}", crate::bytes(e.bytes())),
                                Some(false) => format!("YASim · {}", crate::bytes(e.bytes())),
                                None => crate::bytes(e.bytes()),
                            },
                            State::Failed(_) => t!("hangar.couldnt_be_read").to_string(),
                        };
                        let sub: String = sub.chars().take(44).collect();
                        p.text(rect.left_center() + egui::vec2(10.0, 9.0), Align2::LEFT_CENTER, sub, menu::caption(), DIM);
                        let (tag, col) = match &e.install {
                            Install::Installed => (t!("hangar.installed").to_string(), OK),
                            Install::Downloading(a, b) => (format!("{:.0}%", *a as f64 * 100.0 / (*b).max(1) as f64), ACCENT),
                            Install::Unpacking => (t!("hangar.unpacking").to_string(), ACCENT),
                            Install::Failed(_) => ("!".to_string(), BAD),
                            Install::No => (String::new(), DIM),
                        };
                        p.text(rect.right_top() + egui::vec2(-10.0, 13.0), Align2::RIGHT_CENTER, tag, menu::caption(), col);
                        if resp.clicked() {
                            self.menu.hangar_sel = Some(e.folder.clone());
                        }
                    }
                    crate::padui::follow_focus(ui);
                });
            });
            // The details.
            if let Some(e) = self.menu.hangar_sel.as_ref().and_then(|s| entries.iter().find(|e| &e.folder == s)) {
                ui.scope_builder(column(right), |ui| {
                    egui::Frame::new().fill(Color32::from_white_alpha(6)).corner_radius(menu::R_PANEL).inner_margin(18.0).show(ui, |ui| {
                        ui.set_width(right.width() - 36.0);
                        ui.set_height(h - 36.0);
                        egui::ScrollArea::vertical().id_salt("hangar_detail").auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(right.width() - 36.0);
                            fly = self.detail(ui, e);
                            crate::padui::follow_focus(ui);
                        });
                    });
                });
            }
        }
        fly
    }

    /// One entry of the collection in full. Some(k): fly aircraft k.
    fn detail(&mut self, ui: &mut egui::Ui, e: &hangar::Entry) -> Option<usize> {
        let mut fly = None;
        hangar::want(&e.folder);
        let info = match &e.state {
            State::Ready(i) => Some(i),
            State::Pending => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new(t!("hangar.reading", if e.name.is_empty() { &e.folder } else { &e.name })).color(TEXT));
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                });
                None
            }
            State::Failed(err) => {
                ui.label(RichText::new(&e.folder).font(menu::page_title()).color(Color32::WHITE));
                ui.label(RichText::new(t!("hangar.couldnt_read_it", err)).color(BAD));
                None
            }
        };
        let Some(i) = info else { return None };
        ui.label(RichText::new(&i.name).font(menu::page_title()).color(Color32::WHITE));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(format!("{}  ·  {}", e.folder, crate::bytes(e.bytes()))).font(menu::caption()).color(DIM));
            ui.hyperlink_to(RichText::new(t!("hangar.view_its_files")).font(menu::caption()), hangar::folder_url(&e.folder));
        });
        if !i.author.is_empty() {
            ui.label(RichText::new(t!("hangar.by_2", i.author)).font(menu::body()).color(TEXT));
        }
        if !i.description.is_empty() {
            ui.label(RichText::new(&i.description).font(menu::body()).color(TEXT));
        }
        ui.add_space(10.0);

        // The license.
        menu::heading(ui, t!("hangar.license_2"));
        egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 0, top: 4, bottom: 8 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if i.license_url.is_empty() {
                ui.label(RichText::new(&i.license).font(menu::label()).color(Color32::WHITE));
            } else {
                ui.hyperlink_to(RichText::new(&i.license).font(menu::label()), &i.license_url);
            }
            let from = match i.license_from {
                LicenseFrom::Header => t!("hangar.as_stated_in_the_aircraft").to_string(),
                LicenseFrom::Comment => t!("hangar.as_stated_in_the_aircraft_2").to_string(),
                LicenseFrom::Repository => t!("hangar.the_file_states_no_license").to_string(),
            };
            ui.label(RichText::new(from).font(menu::caption()).color(DIM));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(t!("hangar.flightgears_collection_2")).font(menu::caption()).color(DIM));
                ui.hyperlink_to(RichText::new(hangar::REPO_LICENSE).font(menu::caption()), hangar::REPO_LICENSE_URL);
            });
            if !i.notes.is_empty() {
                ui.add_space(4.0);
                ui.label(menu::section_text(t!("hangar.notes_from_its_authors")).color(DIM));
                let notes: String = i.notes.chars().take(900).collect();
                ui.label(RichText::new(notes).font(menu::caption()).color(DIM));
            }
        });
        ui.add_space(10.0);

        // What it becomes here.
        menu::heading(ui, t!("hangar.in_fable_flight"));
        egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 0, top: 4, bottom: 8 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &i.fit {
                Ok(s) => {
                    egui::Grid::new("fit").num_columns(4).spacing([14.0, 4.0]).show(ui, |ui| {
                        let cell = |ui: &mut egui::Ui, k: &str, v: String| {
                            ui.label(RichText::new(k).font(menu::caption()).color(DIM));
                            ui.label(RichText::new(v).font(menu::caption()).color(Color32::WHITE));
                        };
                        cell(ui, t!("hangar.type"), s.kind.clone());
                        cell(ui, t!("hangar.engines"), s.power.clone());
                        ui.end_row();
                        cell(ui, t!("hangar.mass"), format!("{:.0} kg", s.mass_kg));
                        cell(ui, t!("hangar.span"), format!("{:.1} m", s.span_m));
                        ui.end_row();
                        cell(ui, t!("hangar.stall"), format!("{:.0} kt", s.stall_kt));
                        cell(ui, t!("hangar.top_speed"), t!("hangar.about_kt", s.vmax_kt));
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    let how = if s.own_model {
                        t!("hangar.flies_with_our_flight_model", s.body, s.fdm)
                    } else {
                        t!("hangar.flies_with_our_flight_model_2", s.body, s.fdm)
                    };
                    ui.label(RichText::new(how).font(menu::caption()).color(DIM));
                }
                Err(why) => {
                    ui.label(RichText::new(t!("hangar.cant_be_flown_here_yet")).font(menu::label()).color(BAD));
                    ui.label(RichText::new(why).font(menu::caption()).color(DIM));
                }
            }
        });
        ui.add_space(14.0);

        // Download, fly, remove.
        let key = hangar::key(&e.folder);
        let can = i.fit.is_ok();
        match &e.install {
            Install::No | Install::Failed(_) => {
                if let Install::Failed(err) = &e.install {
                    ui.label(RichText::new(t!("hangar.download_failed", err)).font(menu::caption()).color(BAD));
                }
                let label = t!("hangar.download", crate::bytes(e.bytes()));
                if menu::wide_button(ui, &label, crate::padui::Hint::None, true, can).clicked() {
                    hangar::install(&e.folder);
                }
                ui.label(RichText::new(t!("hangar.downloaded_straight_from_a_flightgear", i.license)).font(menu::caption()).color(DIM));
            }
            Install::Downloading(a, b) => {
                ui.add(egui::ProgressBar::new(*a as f32 / (*b).max(1) as f32).text(t!("hangar.downloading", crate::bytes(*a), crate::bytes(*b))).desired_height(30.0));
            }
            Install::Unpacking => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new(t!("hangar.unpacking_and_reading_its_model")).color(TEXT));
                });
            }
            Install::Installed => {
                let spec = aircraft::list().into_iter().position(|s| s.keys.first() == Some(&key.as_str()));
                match spec {
                    Some(k) => {
                        if menu::wide_button(ui, t!("hangar.select_this_aircraft"), crate::padui::Hint::None, true, true).clicked() {
                            fly = Some(k);
                        }
                    }
                    None => {
                        ui.label(RichText::new(t!("hangar.downloaded_but_it_doesnt_load")).font(menu::caption()).color(BAD));
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("hangar.downloaded")).font(menu::caption()).color(OK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new(t!("hangar.remove")).font(menu::caption())).clicked() {
                            let chosen = aircraft::nth(self.menu.aircraft).keys.first().copied();
                            if let Err(err) = hangar::remove(&e.folder) {
                                log::warn!("removing {}: {err}", e.folder);
                            }
                            // Keep the same aircraft chosen if it is still there.
                            self.menu.aircraft = chosen.and_then(aircraft::by_key).map(aircraft::index_of).unwrap_or(0);
                        }
                    });
                });
            }
        }
        fly
    }
}

/// What kind of aircraft, for the list: built in, or where it came from.
fn kind_line(spec: &Spec) -> String {
    match spec.origin {
        Some(o) if spec.ext.is_some() => t!("hangar.downloaded_2", o.license),
        Some(o) => t!("hangar.drawn_as_the", o.body),
        None => engine_line(spec),
    }
}

/// Under the name on the big view.
fn origin_line(spec: &Spec) -> String {
    match spec.origin {
        Some(o) if spec.ext.is_some() => t!("hangar.downloaded_from_flightgears_collection", o.license),
        Some(o) => t!("hangar.from_flightgears_collection_drawn_as", o.body, o.license),
        None => t!("hangar.built_in").to_string(),
    }
}

/// Its engines in a few words: "180 hp piston", "4 × 252 kN jet".
fn engine_line(spec: &Spec) -> String {
    match &spec.engine {
        aircraft::Engine::Piston(p) => t!("hangar.hp_piston", p.power / 745.7),
        aircraft::Engine::Fan(f) if f.boost > 1.15 => t!("hangar.kn_afterburning", f.n, f.thrust * f.boost / 1000.0),
        aircraft::Engine::Fan(f) => t!("hangar.kn_jet", f.n, f.thrust / 1000.0),
        aircraft::Engine::Rocket(r) => t!("hangar.rocket_engines", r.stages[0].n),
    }
}

/// The aircraft screen's numbers, straight from its `Spec`: (what, value).
fn numbers(spec: &Spec) -> Vec<(&'static str, String)> {
    let kt = |ms: f64| ms * 1.943_84;
    let mut v = vec![(t!("hangar.engines"), engine_line(spec))];
    if spec.rocket().is_none() {
        let top = if spec.mmo > 0.0 { format!("{:.0} kt  ·  M {:.2}", kt(spec.vne), spec.mmo) } else { format!("{:.0} kt", kt(spec.vne)) };
        v.push((t!("hangar.never_exceed_ias"), top));
        v.push((t!("hangar.wingspan"), format!("{:.1} m", spec.b)));
        v.push((t!("hangar.wing_area"), format!("{:.1} m²", spec.wing_area)));
    }
    v.push((t!("hangar.mass_half_fuel"), format!("{} kg", thousands(spec.mass))));
    v.push((t!("hangar.fuel_full_tanks"), format!("{} kg", thousands(spec.fuel_kg))));
    v
}

/// A whole number with its thousands spaced out: "290 000".
fn thousands(v: f64) -> String {
    let d = format!("{:.0}", v.abs());
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    if v < -0.5 { format!("-{out}") } else { out }
}

/// One line of text cut to a width, with an ellipsis where it is cut.
fn one_line(p: &egui::Painter, text: &str, size: f32, col: Color32, w: f32) -> std::sync::Arc<egui::Galley> {
    one_line_font(p, text, egui::FontId::proportional(size), col, w)
}

/// `one_line` in a given font.
fn one_line_font(p: &egui::Painter, text: &str, font: egui::FontId, col: Color32, w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(font, col));
    job.wrap = egui::text::TextWrapping { max_width: w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.layout_job(job)
}

/// A small label on a picture.
fn badge(p: &egui::Painter, at: egui::Pos2, text: &str) {
    let g = p.layout_no_wrap(text.to_uppercase(), menu::bold(12.5), TEXT);
    let r = egui::Rect::from_min_size(at, g.size() + egui::vec2(14.0, 4.0));
    p.rect_filled(r, menu::R_WIDGET, Color32::from_rgba_premultiplied(0, 0, 0, 150));
    p.rect_stroke(r, menu::R_WIDGET, Stroke::new(1.0, menu::LINE_HI), egui::StrokeKind::Inside);
    p.galley(r.min + egui::vec2(7.0, 2.0), g, TEXT);
}
