//! The settings screen: one full-window page shared by the start screen and
//! the pause menu. A sidebar of sections on the left (painted icons, no
//! glyphs the bundled font might not have), the chosen section's cards on
//! the right in a column of at most `MAX_W` points.
//!
//! Entry point: [`show`] (or `Game::settings_screen`). It draws the screen
//! over everything and returns true when it is closed with its Back button;
//! Esc and the gamepad's B are handled by the caller's key handling (they
//! close it the same way). The section shown is `Game::settings_sec`.
//!
//! Every setting lives here except the ones that only make sense in flight
//! (failures, in the pause menu). The values themselves are saved where
//! they always were: settings.json (settings.rs, compared each frame by
//! `keep_settings`), graphics.json (graphics.rs), bindings (bind.rs) and the
//! Google key (gmaps.rs).
use crate::menu::{self, ACCENT, DIM, TEXT};
use crate::padui::{self, Hint};
use crate::{avatar, bind, geo, gmaps, i18n, mp, mp_proto, padmap, t, tiles3d, Game};
use egui::{pos2, vec2, Align, Align2, Color32, Layout, Painter, Pos2, Rect, RichText, Sense, Stroke, Ui};

/// The widest the content column gets, however wide the window.
pub const MAX_W: f32 = 780.0;

/// A section of the settings screen.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    Graphics,
    Sound,
    Camera,
    Controls,
    Map,
    Multiplayer,
}

impl Section {
    pub const ALL: [Section; 7] = [Section::General, Section::Graphics, Section::Sound, Section::Camera, Section::Controls, Section::Map, Section::Multiplayer];

    /// The name the command line goes by (`--settings graphics`, `--pause-menu settings:graphics`).
    pub fn name(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Graphics => "graphics",
            Section::Sound => "sound",
            Section::Camera => "camera",
            Section::Controls => "controls",
            Section::Map => "map",
            Section::Multiplayer => "multiplayer",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Section::General => t!("settings.general"),
            Section::Graphics => t!("settings.graphics"),
            Section::Sound => t!("settings.sound"),
            Section::Camera => t!("settings.camera_and_view"),
            Section::Controls => t!("settings.controls"),
            Section::Map => t!("settings.map_and_data"),
            Section::Multiplayer => t!("settings.multiplayer"),
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Section::General => t!("settings.language_the_hud_the_minimap"),
            Section::Graphics => t!("settings.how_hard_the_renderer_works"),
            Section::Sound => t!("settings.engine_wind_warnings_and_the"),
            Section::Camera => t!("settings.where_the_camera_starts_and"),
            Section::Controls => t!("settings.mouse_joysticks_pedals_gamepads_and"),
            Section::Map => t!("settings.map_imagery_googles_3d_cities"),
            Section::Multiplayer => t!("settings.fly_with_the_other_players"),
        }
    }

    pub fn parse(s: &str) -> Option<Section> {
        Some(match s.trim().to_lowercase().as_str() {
            "general" | "game" | "hud" | "language" => Section::General,
            "graphics" | "gfx" | "video" => Section::Graphics,
            "sound" | "audio" => Section::Sound,
            "camera" | "view" | "cam" => Section::Camera,
            "controls" | "bindings" | "input" | "keys" => Section::Controls,
            "map" | "data" | "imagery" | "google" => Section::Map,
            "multiplayer" | "mp" | "online" | "network" => Section::Multiplayer,
            _ => return None,
        })
    }

    /// The section before or after this one, round and round.
    pub fn cycle(self, next: bool) -> Section {
        let n = Section::ALL.len();
        let i = Section::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Section::ALL[(i + if next { 1 } else { n - 1 }) % n]
    }

    /// A small line icon for the sidebar, `s` points across, centred on `c`.
    pub(crate) fn icon(self, p: &Painter, c: Pos2, s: f32, col: Color32) {
        let st = Stroke::new(1.6, col);
        let u = s / 2.0;
        match self {
            // three sliders
            Section::General => {
                for (i, k) in [(-1.0, 0.3), (0.0, -0.4), (1.0, 0.15)] {
                    let y = c.y + i * u * 0.65;
                    p.line_segment([pos2(c.x - u, y), pos2(c.x + u, y)], st);
                    p.circle_filled(pos2(c.x + k * u, y), u * 0.22, col);
                }
            }
            // a monitor on its stand
            Section::Graphics => {
                let r = Rect::from_center_size(c - vec2(0.0, u * 0.15), vec2(s, s * 0.66));
                p.rect_stroke(r, 1.5, st, egui::StrokeKind::Middle);
                p.line_segment([pos2(c.x, r.bottom()), pos2(c.x, r.bottom() + u * 0.3)], st);
                p.line_segment([pos2(c.x - u * 0.45, r.bottom() + u * 0.32), pos2(c.x + u * 0.45, r.bottom() + u * 0.32)], st);
            }
            // a speaker and two waves
            Section::Sound => {
                let x0 = c.x - u;
                let body = vec![pos2(x0, c.y - u * 0.3), pos2(x0 + u * 0.4, c.y - u * 0.3), pos2(x0 + u * 0.95, c.y - u * 0.8), pos2(x0 + u * 0.95, c.y + u * 0.8), pos2(x0 + u * 0.4, c.y + u * 0.3), pos2(x0, c.y + u * 0.3)];
                p.add(egui::Shape::closed_line(body, st));
                for r in [0.45, 0.85] {
                    let pts: Vec<Pos2> = (0..=8).map(|i| {
                        let a = (i as f32 / 8.0 - 0.5) * 1.6;
                        pos2(c.x + u * 0.05 + a.cos() * r * u, c.y + a.sin() * r * u)
                    }).collect();
                    p.line(pts, st);
                }
            }
            // a camera body and its lens
            Section::Camera => {
                let r = Rect::from_center_size(c + vec2(0.0, u * 0.1), vec2(s, s * 0.68));
                p.rect_stroke(r, 2.0, st, egui::StrokeKind::Middle);
                p.line_segment([pos2(c.x - u * 0.35, r.top() - u * 0.2), pos2(c.x + u * 0.35, r.top() - u * 0.2)], st);
                p.circle_stroke(r.center(), u * 0.38, st);
            }
            // a gamepad: its outline, a cross and two buttons
            Section::Controls => {
                let r = Rect::from_center_size(c, vec2(s * 1.05, s * 0.62));
                p.rect_stroke(r, u * 0.3, st, egui::StrokeKind::Middle);
                let d = c - vec2(u * 0.48, 0.0);
                p.line_segment([d - vec2(u * 0.22, 0.0), d + vec2(u * 0.22, 0.0)], st);
                p.line_segment([d - vec2(0.0, u * 0.22), d + vec2(0.0, u * 0.22)], st);
                p.circle_filled(c + vec2(u * 0.38, u * 0.1), u * 0.11, col);
                p.circle_filled(c + vec2(u * 0.6, -u * 0.12), u * 0.11, col);
            }
            // a globe: its rim, a meridian and the equator
            Section::Map => {
                p.circle_stroke(c, u * 0.95, st);
                p.line_segment([c - vec2(u * 0.95, 0.0), c + vec2(u * 0.95, 0.0)], st);
                let pts: Vec<Pos2> = (0..=16).map(|i| {
                    let a = (i as f32 / 16.0) * std::f32::consts::TAU;
                    c + vec2(a.cos() * u * 0.4, a.sin() * u * 0.95)
                }).collect();
                p.line(pts, st);
            }
            // two people, one a step behind the other
            Section::Multiplayer => {
                for (dx, k) in [(u * 0.42, 0.82), (-u * 0.22, 1.0)] {
                    let h = c + vec2(dx, -u * 0.38 * k);
                    p.circle_stroke(h, u * 0.3 * k, st);
                    let pts: Vec<Pos2> = (0..=10).map(|i| {
                        let a = std::f32::consts::PI * (1.0 + i as f32 / 10.0);
                        pos2(c.x + dx + a.cos() * u * 0.62 * k, c.y + u * 0.88 + a.sin() * u * 0.7 * k)
                    }).collect();
                    p.line(pts, st);
                }
            }
        }
    }
}

// ---- building blocks for the sections (graphics.rs uses them too)

/// A titled group of settings: a faint card with a small accent title.
pub fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(menu::SURFACE)
        .stroke(Stroke::new(1.0, menu::LINE))
        .corner_radius(menu::R_PANEL)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 16, bottom: 14 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(menu::section_text(title));
            ui.add_space(12.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            body(ui);
        });
    ui.add_space(16.0);
}

/// One `label | control` row: the label (and a short dim hint under it) in
/// a fixed left column, the control to its right.
pub fn row(ui: &mut Ui, label: &str, hint: &str, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let lw = (w * 0.34).clamp(150.0, 250.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(lw, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(lw - 12.0);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(3.0);
            ui.add(egui::Label::new(RichText::new(label).font(menu::label()).color(TEXT)).wrap());
            if !hint.is_empty() {
                ui.add(egui::Label::new(RichText::new(hint).font(menu::caption()).color(DIM)).wrap());
            }
        });
        ui.vertical(|ui| {
            ui.set_width((w - lw - 8.0).max(100.0));
            add(ui);
        });
    });
    ui.add_space(8.0);
}

/// The first cell of a grid row, as wide as a `row`'s label column so
/// grids line up with the rows around them.
fn grid_label(ui: &mut Ui, text: &str, card_w: f32) {
    let lw = (card_w * 0.34).clamp(150.0, 250.0) - 16.0;
    ui.horizontal(|ui| {
        ui.set_min_width(lw);
        ui.label(RichText::new(text).font(menu::label()).color(TEXT));
    });
}

/// A dim explanatory line across the card.
pub fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(menu::caption()).color(DIM)).wrap());
}

/// Draws the settings screen for `game` (see the module docs); true when
/// its Back button was pressed. `in_flight`: over the paused game (dimmed)
/// rather than the start screen.
pub fn show(ctx: &egui::Context, game: &mut Game, in_flight: bool) -> bool {
    game.settings_screen(ctx, in_flight)
}

impl Game {
    pub(crate) fn settings_screen(&mut self, ctx: &egui::Context, in_flight: bool) -> bool {
        let screen = ctx.content_rect();
        let mut closed = false;
        egui::Area::new(egui::Id::unique("settings_screen")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            // The backdrop takes every click; the game (or globe) behind is all but hidden.
            ui.interact(screen, ui.scope_id().with("backdrop"), Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(screen, 0.0, if in_flight { Color32::from_rgba_premultiplied(4, 6, 9, 226) } else { menu::BG });
            let side_w = if screen.width() < 1000.0 { 220.0 } else { 264.0 };
            let side = Rect::from_min_size(screen.min, vec2(side_w, screen.height()));
            p.rect_filled(side, 0.0, Color32::from_black_alpha(110));
            p.vline(side.right(), side.y_range(), Stroke::new(1.0, menu::LINE_HI));

            // The sidebar: title, the sections, Back at the foot.
            let mut s = ui.new_child(egui::UiBuilder::new().max_rect(side.shrink2(vec2(16.0, 28.0))).layout(Layout::top_down(Align::Min)));
            s.add_space(10.0);
            menu::screen_heading(&mut s, t!("settings.settings"));
            s.add_space(24.0);
            s.spacing_mut().item_spacing.y = 2.0;
            // the chosen tab's marker slides to it.
            let marker = s.painter().add(egui::Shape::Noop);
            let mut on_rect = None;
            for sec in Section::ALL {
                let r = self.settings_tab(&mut s, sec);
                if self.settings_sec == sec {
                    on_rect = Some(r.rect);
                }
                if r.clicked() && self.settings_sec != sec {
                    self.settings_sec = sec;
                }
            }
            if let Some(r) = on_rect {
                let y = ctx.animate_value_with_time(s.scope_id().with("tab_marker"), r.top(), 0.2);
                let m = Rect::from_min_size(pos2(r.left(), y), r.size());
                s.painter().set(marker, egui::Shape::rect_filled(m, menu::R_WIDGET, Color32::from_white_alpha(14)));
                let bar = Rect::from_center_size(pos2(m.left() + 1.5, m.center().y), vec2(3.0, m.height() * 0.5));
                s.painter().add(egui::Shape::rect_filled(bar, 1.0, ACCENT));
            }
            let foot = Rect::from_min_max(pos2(side.left() + 16.0, side.bottom() - 100.0), pos2(side.right() - 16.0, side.bottom() - 24.0));
            let mut f = ui.new_child(egui::UiBuilder::new().max_rect(foot).layout(Layout::bottom_up(Align::Min)));
            if padui::active() {
                padui::prompts(&mut f, &[("LB / RB", crate::tr_en("section")), ("B", crate::tr_en("back"))], 18.0, 12.0, DIM);
            } else {
                f.label(RichText::new(t!("settings.changes_are_saved_as_you")).font(menu::caption()).color(DIM));
            }
            f.add_space(6.0);
            let back = if in_flight { t!("settings.back_to_the_pause_menu") } else { t!("settings.back") };
            if menu::wide_button(&mut f, back, Hint::of("Esc", "B"), false, true).clicked() {
                closed = true;
            }

            // The content: the section's title and its cards, in a centred column.
            let area = Rect::from_min_max(pos2(side.right() + 1.0, screen.top()), screen.max);
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Min)));
            let col_w = (area.width() - 96.0).clamp(320.0, MAX_W);
            let left = ((area.width() - col_w) * 0.5).max(24.0);
            egui::ScrollArea::vertical().id_salt(("settings_scroll", self.settings_sec.name())).auto_shrink([false, false]).show(&mut c, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add_space(left);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        ui.add_space(44.0);
                        let sec = self.settings_sec;
                        menu::page_heading(ui, sec.label(), sec.blurb());
                        ui.add_space(24.0);
                        ui.spacing_mut().slider_width = (col_w * 0.36).clamp(160.0, 300.0);
                        match sec {
                            Section::General => self.set_general(ui),
                            Section::Graphics => self.set_graphics(ui),
                            Section::Sound => self.set_sound(ui),
                            Section::Camera => self.set_camera(ui, in_flight),
                            Section::Controls => self.set_controls(ui),
                            Section::Map => self.set_map(ui),
                            Section::Multiplayer => self.set_multiplayer(ui, in_flight),
                        }
                        ui.add_space(40.0);
                    });
                });
                padui::follow_focus(ui);
            });
            ui.expand_to_include_rect(screen);
        });
        closed
    }

    /// One sidebar tab: its icon and name, the chosen one marked in the accent colour.
    fn settings_tab(&mut self, ui: &mut Ui, sec: Section) -> egui::Response {
        let on = self.settings_sec == sec;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, sec.label()));
        if on {
            self.pad_home_focus(&resp);
        }
        let lit = menu::ease(ui.ctx(), resp.id, (resp.hovered() || resp.has_focus()) && !on);
        let pick = ui.ctx().animate_bool_with_time(resp.id.with("on"), on, 0.2);
        let p = ui.painter();
        if lit > 0.0 {
            p.rect_filled(rect, menu::R_WIDGET, Color32::from_white_alpha((7.0 * lit) as u8));
        }
        let col = menu::mix(menu::mix(DIM, TEXT, lit), Color32::WHITE, pick);
        sec.icon(p, pos2(rect.left() + 26.0, rect.center().y), 18.0, menu::mix(col, ACCENT, pick));
        p.text(pos2(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, sec.label(), menu::label(), col);
        resp
    }

    fn set_general(&mut self, ui: &mut Ui) {
        card(ui, t!("settings.language"), |ui| {
            row(ui, t!("settings.interface_language"), "", |ui| self.language_picker(ui));
        });
        card(ui, t!("settings.hud_and_minimap"), |ui| {
            row(ui, "HUD", t!("settings.key_h_in_flight"), |ui| {
                menu::switch(ui, &mut self.show_hud);
            });
            row(ui, t!("settings.hud_in_the_cockpit"), t!("settings.off_the_cockpits_own_instruments"), |ui| {
                ui.add_enabled_ui(self.show_hud, |ui| menu::switch(ui, &mut self.hud_cockpit));
            });
            row(ui, t!("settings.flight_path_marker"), t!("settings.a_ring_on_the_hud"), |ui| {
                ui.add_enabled_ui(self.show_hud, |ui| menu::switch(ui, &mut self.path_marker));
            });
            row(ui, t!("settings.minimap"), t!("settings.key_left_of_1_in"), |ui| {
                menu::switch(ui, &mut self.minimap.visible);
            });
            row(ui, t!("settings.minimap_size"), t!("settings.it_fits_the_window_and"), |ui| {
                if ui.add(egui::Slider::new(&mut self.minimap.size.x, 240.0..=640.0).step_by(10.0).suffix(" px")).changed() {
                    self.minimap.size.y = self.minimap.size.x;
                }
            });
            row(ui, t!("settings.minimap_style"), "", |ui| {
                menu::segments(ui, |ui| {
                    menu::seg(ui, &mut self.minimap.plan, false, t!("settings.satellite"));
                    menu::seg(ui, &mut self.minimap.plan, true, t!("settings.plan"));
                });
            });
            row(ui, t!("settings.minimap_orientation"), "", |ui| {
                menu::segments(ui, |ui| {
                    menu::seg(ui, &mut self.minimap.heading_up, false, t!("settings.north_up"));
                    menu::seg(ui, &mut self.minimap.heading_up, true, t!("settings.heading_up"));
                });
            });
            row(ui, t!("settings.landing_guidance"), t!("settings.approach_arrows_and_tunnel"), |ui| {
                menu::switch(ui, &mut self.minimap.landing_guidance);
            });
        });
        card(ui, t!("settings.mode"), |ui| {
            row(ui, t!("settings.arcade_mode"), t!("settings.missiles_boost_on_any_aircraft"), |ui| {
                menu::switch(ui, &mut self.arcade);
            });
            row(ui, t!("settings.ground_first_person"), t!("settings.the_view_on_foot_starts"), |ui| {
                menu::switch(ui, &mut self.ground_first_person);
            });
            row(ui, t!("settings.avatar_style"), "", |ui| {
                menu::segments(ui, |ui| {
                    for style in 0..avatar::STYLES as u8 {
                        menu::seg(ui, &mut self.avatar_style, style, &format!("{}", style + 1));
                    }
                });
            });
            row(ui, t!("settings.autosave"), t!("settings.save_your_place_in_the"), |ui| {
                menu::switch(ui, &mut self.autosave);
            });
        });
        card(ui, t!("settings.air_traffic"), |ui| {
            row(ui, t!("settings.live_air_traffic"), t!("settings.real_aircraft_around_you_from"), |ui| {
                let mut on = self.live.enabled();
                if menu::switch(ui, &mut on).changed() {
                    self.live.set_enabled(on);
                }
            });
        });
        let t = if padui::active() { t!("settings.in_flight_view_switches_the") } else { t!("settings.shortcuts_in_flight_u_sound") };
        note(ui, t);
    }

    fn set_graphics(&mut self, ui: &mut Ui) {
        let before = self.gfx;
        let size = self.window.inner_size();
        card(ui, t!("settings.preset"), |ui| self.gfx.preset_ui(ui));
        card(ui, t!("settings.quality"), |ui| self.gfx.quality_ui(ui));
        card(ui, t!("settings.display"), |ui| self.gfx.display_ui(ui, (size.width, size.height), self.fps));
        let cx = crate::graphics::UpscaleCx { gpu: &self.r.gpu, window: (size.width, size.height), active: self.r.up.active(), fsr4_blocked: crate::fsr4::blocked(&self.r.gpu, self.r.up.fsr4_error()), fsr4_provider: self.r.up.fsr4_provider().map(str::to_owned) };
        card(ui, t!("settings.upscaling"), |ui| self.gfx.upscaler_ui(ui, &cx));
        card(ui, t!("settings.scenery"), |ui| {
            use crate::snow::Mode;
            row(ui, t!("settings.snow_on_the_ground"), t!("settings.automatic_from_the_date_the"), |ui| {
                menu::segments(ui, |ui| {
                    menu::seg(ui, &mut self.snow, Mode::Auto, t!("settings.automatic"));
                    menu::seg(ui, &mut self.snow, Mode::Off, t!("settings.off"));
                    menu::seg(ui, &mut self.snow, Mode::Always, t!("settings.always"));
                });
            });
        });
        let on = if self.src.google.photoreal() { t!("settings.on") } else { t!("settings.off_2") };
        note(ui, &t!("settings.googles_photorealistic_3d_cities_have", on));
        if self.gfx != before {
            self.gfx.save();
        }
    }

    fn set_sound(&mut self, ui: &mut Ui) {
        card(ui, t!("settings.sound"), |ui| {
            row(ui, t!("settings.sound"), t!("settings.key_u_in_flight"), |ui| {
                menu::switch(ui, &mut self.sound_on);
            });
            row(ui, t!("settings.volume"), "", |ui| {
                ui.add_enabled(self.sound_on, egui::Slider::new(&mut self.volume, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            });
        });
    }

    fn set_camera(&mut self, ui: &mut Ui, in_flight: bool) {
        card(ui, t!("settings.camera"), |ui| {
            let hint = if in_flight { t!("settings.c_switches_it_in_flight") } else { t!("settings.the_view_you_start_in") };
            row(ui, t!("settings.camera"), hint, |ui| {
                let previous = self.cam_mode;
                menu::segments(ui, |ui| {
                    menu::seg(ui, &mut self.cam_mode, 0, t!("settings.chase"));
                    menu::seg(ui, &mut self.cam_mode, crate::settings::TAIL_VIEW, t!("settings.fixed_rear"));
                    menu::seg(ui, &mut self.cam_mode, 1, "Cockpit");
                });
                if self.cam_mode == crate::settings::TAIL_VIEW && previous != self.cam_mode {
                    self.view_before_tail = previous;
                    self.cancel_cockpit_input();
                }
            });
            row(ui, t!("settings.cockpit_field_of_view"), t!("settings.the_mouse_wheel_zooms_for"), |ui| {
                ui.add(egui::Slider::new(&mut self.cockpit_fov, 35.0..=85.0).custom_formatter(|v, _| format!("{v:.0}° vertical")));
            });
            row(ui, t!("settings.invert_cockpit_view"), t!("settings.invert_view_hint"), |ui| {
                menu::switch(ui, &mut self.invert_look_cockpit);
            });
            row(ui, t!("settings.invert_outside_horizontal"), t!("settings.invert_view_hint"), |ui| {
                menu::switch(ui, &mut self.invert_look_outside_x);
            });
            row(ui, t!("settings.invert_outside_vertical"), t!("settings.invert_view_hint"), |ui| {
                menu::switch(ui, &mut self.invert_look_outside_y);
            });
        });
    }

    fn set_controls(&mut self, ui: &mut Ui) {
        self.binds_capture_banner(ui);
        card(ui, t!("settings.mouse"), |ui| {
            row(ui, t!("settings.mouse_look_speed"), t!("settings.dragging_to_look_around"), |ui| {
                ui.add(egui::Slider::new(&mut self.look_sens, 0.25..=3.0).logarithmic(true).custom_formatter(|v, _| format!("{v:.2}×")));
            });
        });
        // Which controller's keys are being edited: the same physical key
        // means different things on foot, in a car and in flight.
        card(ui, t!("settings.controls_context"), |ui| {
            menu::segments(ui, |ui| {
                for ctx in bind::Context::ALL {
                    menu::seg(ui, &mut self.bind_ctx, ctx, ctx.label());
                }
            });
            note(ui, t!("settings.controls_context_hint"));
            if self.bind_ctx == bind::Context::Foot {
                ui.add_space(6.0);
                // The pad's own on-foot layout.
                bind_grid(ui, "pad_foot", |ui| {
                    for (k, w) in padmap::foot_rows() {
                        ui.label(RichText::new(k).font(menu::bold(12.0)).color(ACCENT));
                        ui.label(RichText::new(w).font(menu::caption()).color(TEXT));
                        ui.end_row();
                    }
                });
            } else if self.bind_ctx == bind::Context::Car {
                ui.add_space(6.0);
                // The pad's own in-car layout.
                bind_grid(ui, "pad_car", |ui| {
                    for (k, w) in padmap::car_rows() {
                        ui.label(RichText::new(k).font(menu::bold(12.0)).color(ACCENT));
                        ui.label(RichText::new(w).font(menu::caption()).color(TEXT));
                        ui.end_row();
                    }
                });
            }
        });
        card(ui, t!("settings.connected_devices"), |ui| self.binds_devices(ui));
        // One search over every bindings table below (and the key reference).
        let words = menu::search_box(ui, "bindings", t!("menu.search_bindings"), ui.available_width().min(460.0));
        ui.add_space(6.0);
        card(ui, t!("settings.gamepad"), |ui| self.binds_pad(ui, &words));
        card(ui, t!("settings.joystick_axes"), |ui| self.binds_axes(ui, &words));
        card(ui, t!("settings.buttons_joystick_throttle_wheel"), |ui| self.binds_buttons(ui, &words));
        card(ui, t!("settings.keyboard"), |ui| self.binds_keys(ui, &words));
        if ui.add_enabled(self.binds.capture.is_none(), egui::Button::new(t!("settings.reset_all_bindings"))).clicked() {
            self.binds = bind::Bindings::default();
            self.keys.clear();
        }
        ui.add_space(14.0);
        card(ui, t!("settings.key_reference"), |ui| {
            let t = if padui::active() { t!("settings.in_flight_y_view_shows") } else { t!("settings.in_flight_tab_shows_the") };
            note(ui, t);
            ui.add_space(6.0);
            menu::controls_list(ui);
        });
    }

    fn set_map(&mut self, ui: &mut Ui) {
        let g = self.src.google.clone();
        let has_key = !self.gkey.trim().is_empty();
        let need_key = t!("settings.enter_a_google_maps_platform");
        card(ui, t!("settings.map_imagery"), |ui| {
            row(ui, t!("settings.google_api_key"), t!("settings.map_tiles_api_billed_to"), |ui| {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.gkey).password(!self.gkey_show).hint_text(t!("settings.google_maps_platform_key")).desired_width((ui.available_width() - 60.0).min(380.0)));
                    if r.lost_focus() || (r.changed() && self.gkey.trim().is_empty()) {
                        g.set_key(&self.gkey);
                    }
                    if ui.button(if self.gkey_show { t!("settings.hide") } else { t!("settings.show") }).clicked() {
                        self.gkey_show = !self.gkey_show;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to(RichText::new(t!("settings.how_to_get_a_key")).font(menu::caption()), "https://developers.google.com/maps/documentation/tile/get-api-key");
                    ui.label(RichText::new("·").font(menu::caption()).color(DIM));
                    ui.hyperlink_to(RichText::new(t!("settings.cloud_console_credentials")).font(menu::caption()), "https://console.cloud.google.com/google/maps-apis/credentials");
                });
            });
            row(ui, t!("settings.satellite_imagery"), if has_key { t!("settings.google_maps") } else { need_key }, |ui| {
                let mut on = g.enabled();
                if ui.add_enabled_ui(has_key, |ui| menu::switch(ui, &mut on)).inner.changed() {
                    g.set_key(&self.gkey);
                    g.set_enabled(on);
                }
                note(ui, if on { t!("settings.terrain_and_roofs_take_googles") } else { t!("settings.off_the_free_default_imagery") });
            });
            let status = g.status.lock().clone();
            if !status.is_empty() {
                ui.label(RichText::new(status.chars().take(160).collect::<String>()).font(menu::caption()).color(Color32::from_rgb(255, 150, 70)));
            }
        });
        card(ui, t!("settings.photorealistic_3d_cities"), |ui| {
            let mut photo = g.photoreal();
            row(ui, t!("settings.google_3d_tiles"), if has_key { t!("settings.beta") } else { need_key }, |ui| {
                if ui.add_enabled_ui(has_key, |ui| menu::switch(ui, &mut photo)).inner.changed() {
                    g.set_key(&self.gkey);
                    g.set_photoreal(photo);
                }
            });
            let detail = g.detail();
            let zone_km = tiles3d::Params::of(detail).zone_r / 1000.0;
            row(ui, t!("settings.3d_detail"), "", |ui| {
                ui.add_enabled_ui(photo, |ui| {
                    let mut d = detail;
                    menu::segments(ui, |ui| {
                        for c in gmaps::Detail::ALL {
                            let name = match c {
                                gmaps::Detail::Low => t!("settings.low"),
                                gmaps::Detail::Medium => t!("settings.medium"),
                                gmaps::Detail::High => t!("settings.high"),
                                gmaps::Detail::Ultra => "Ultra",
                            };
                            menu::seg(ui, &mut d, c, name);
                        }
                    });
                    if d != detail {
                        g.set_detail(d);
                    }
                    note(ui, &t!("settings.one_billed_session_per_flight", crate::detail_hint(detail)));
                });
            });
            note(ui, &if photo { t!("settings.googles_photogrammetry_meshes_replace_th", zone_km = zone_km) } else { t!("settings.off_our_own_3d_world").to_string() });
        });
        card(ui, t!("settings.google_usage"), |ui| {
            let u = g.usage.lock().clone();
            let req = |n: u64| i18n::plural(n, "settings.requests");
            let line = |ui: &mut Ui, k: &str, v: String| row(ui, k, "", |ui| {
                ui.label(RichText::new(v).font(menu::caption()).color(TEXT));
            });
            line(ui, t!("settings.this_session"), format!("{} · {}", req(u.session_requests as u64), crate::bytes(u.session_bytes)));
            line(ui, t!("settings.imagery"), t!("settings.tiles_downloaded_from_disk", u.session_tiles, u.session_cached));
            if u.session_roots > 0 || u.total_roots > 0 {
                let ses = i18n::plural(u.session_roots as u64, "settings.sessions");
                line(ui, "   3D", t!("settings.billed_tiles_from_disk", ses, u.session_3d, u.session_3d_cached));
            }
            if u.session_errors > 0 {
                line(ui, t!("settings.failed"), format!("{}", u.session_errors));
            }
            line(ui, &t!("settings.month", u.month), format!("{} · {}", req(u.month_requests as u64), crate::bytes(u.month_bytes)));
            line(ui, t!("settings.all_time"), format!("{} · {}", req(u.total_requests as u64), crate::bytes(u.total_bytes)));
            if u.total_roots > 0 {
                line(ui, t!("settings.3d_sessions"), t!("settings.this_month_all_time", u.month_roots, u.total_roots));
            }
            note(ui, t!("settings.counts_only_calls_that_reach"));
        });
        card(ui, t!("settings.real_world_data"), |ui| {
            row(ui, t!("settings.real_weather"), t!("settings.metar_else_open_meteo_radar"), |ui| {
                menu::switch(ui, &mut self.wx.live);
            });
            row(ui, t!("settings.real_time"), t!("settings.the_sun_where_you_are"), |ui| {
                menu::switch(ui, &mut self.wx.real_time);
            });
            row(ui, t!("settings.live_air_traffic"), t!("settings.real_aircraft_around_you_from"), |ui| {
                let mut on = self.live.enabled();
                if menu::switch(ui, &mut on).changed() {
                    self.live.set_enabled(on);
                }
            });
        });
    }

    // ---- multiplayer (mp.rs)

    /// Opens the settings screen at `sec`; its Back button (and Esc) return to `back`.
    pub(crate) fn open_settings(&mut self, sec: Section, back: crate::menu::Screen) {
        self.settings_sec = sec;
        self.menu.set_back = back;
        self.menu.screen = crate::menu::Screen::Settings;
        self.pad_home = true;
    }

    /// Where multiplayer stands in a few words, and its colour (the
    /// settings, the main menu's Multiplayer bar and the globe use it).
    pub(crate) fn mp_line(&self) -> (Color32, String) {
        if !self.mp_on {
            return (DIM, t!("settings.not_connected").to_string());
        }
        let server = self.mp.as_ref().map(|m| self.mp_server_name(&m.server)).unwrap_or_default();
        match self.mp.as_ref().map(|m| m.status()) {
            None => (ACCENT, t!("settings.looking_for_the_fastest_server").to_string()),
            Some(mp::Status::Connecting) => (ACCENT, t!("settings.connecting_to", server)),
            Some(mp::Status::Error(e)) => (menu::BAD, t!("settings.no_connection", e.chars().take(60).collect::<String>())),
            Some(mp::Status::Connected) => (menu::OK, t!("settings.connected_to", server)),
        }
    }

    /// The same in a word or two, for the main menu's Multiplayer bar.
    pub(crate) fn mp_short(&self) -> (Color32, String) {
        let (col, _) = self.mp_line();
        let text = if let Some(n) = self.mp_flying() {
            n
        } else {
            match (self.mp_on, self.mp.as_ref().map(|m| m.status())) {
                (false, _) => t!("settings.off_3").to_string(),
                (true, Some(mp::Status::Connected)) => t!("settings.connected").to_string(),
                (true, Some(mp::Status::Error(_))) => t!("settings.no_connection_2").to_string(),
                (true, _) => t!("settings.connecting").to_string(),
            }
        };
        (col, text)
    }

    /// How many fly, as the menus show it ("12 pilots flying"), once the relay has said.
    pub(crate) fn mp_flying(&self) -> Option<String> {
        let connected = self.mp.as_ref().is_some_and(|m| m.status() == mp::Status::Connected);
        (connected && self.mp_world_known).then(|| i18n::plural(self.mp_world_total as u64, "mp.pilots_flying"))
    }

    fn set_multiplayer(&mut self, ui: &mut Ui, in_flight: bool) {
        card(ui, t!("settings.connection"), |ui| {
            // Where it stands, and the one button that changes it.
            let (col, line) = self.mp_line();
            let sub = if !self.mp_on {
                t!("settings.connect_to_see_the_other").to_string()
            } else if in_flight {
                i18n::plural(self.mp_players.len() as u64, "mp.pilots_nearby")
            } else if let Some(n) = self.mp_flying() {
                t!("settings.on_the_network_they_are", n)
            } else if self.mp.as_ref().is_some_and(|m| m.status() == mp::Status::Connected) {
                t!("settings.this_server_does_not_send").to_string()
            } else {
                String::new()
            };
            ui.horizontal(|ui| {
                let bw = 190.0;
                ui.allocate_ui_with_layout(vec2((ui.available_width() - bw - 16.0).max(120.0), 46.0), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, col);
                        ui.add(egui::Label::new(RichText::new(line).font(menu::label()).color(TEXT)).truncate());
                    });
                    if !sub.is_empty() {
                        ui.add(egui::Label::new(RichText::new(sub).font(menu::caption()).color(DIM)).wrap());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.allocate_ui(vec2(bw, 46.0), |ui| {
                        let (text, primary) = if self.mp_on { (t!("settings.disconnect"), false) } else { (t!("settings.connect"), true) };
                        if menu::wide_button(ui, text, Hint::None, primary, true).clicked() {
                            self.mp_on = !self.mp_on;
                            self.mp_held = false;
                        }
                    });
                });
            });
            ui.add_space(10.0);
            row(ui, t!("settings.your_name"), t!("settings.shown_to_the_others_left"), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.mp_name).desired_width(240.0).char_limit(mp_proto::NAME_MAX).hint_text(t!("settings.your_name")));
            });
            row(ui, t!("settings.server"), t!("settings.the_servers_form_one_network"), |ui| self.mp_server_picker(ui));
        });
        if !in_flight && self.mp_on && !self.mp_world.is_empty() {
            card(ui, t!("settings.flying_now"), |ui| self.mp_pilots(ui));
        }
        if in_flight && !self.mp_players.is_empty() {
            card(ui, t!("settings.nearby"), |ui| {
                let me = self.plane.pos;
                for p in self.mp_players.iter().take(12) {
                    let ac = mp_proto::AIRCRAFT.get(p.aircraft as usize).copied().unwrap_or("?");
                    let alt = geo::pos_to_lla(p.pos).2 * 3.28084;
                    row(ui, &p.name, ac, |ui| {
                        ui.label(RichText::new(format!("{:.1} km  ·  {:.0} ft  ·  {:.0} kt", (p.pos - me).length() / 1000.0, alt, p.vel.length() * 1.94384)).font(menu::data()).color(TEXT));
                    });
                }
            });
        }
        note(ui, t!("settings.only_while_you_are_connected"));
    }

    /// The pilots flying on the network (start screen): name, aircraft,
    /// where and how high; Show turns the globe to them.
    fn mp_pilots(&mut self, ui: &mut Ui) {
        let mut look: Option<(f64, f64)> = None;
        let shown = 16;
        for s in self.mp_world.iter().take(shown) {
            let ac = mp_proto::AIRCRAFT.get(s.aircraft as usize).copied().unwrap_or("?");
            let (lat, lon) = (s.lat_deg(), s.lon_deg());
            let place = match crate::menu::Menu::near_city(lat, lon) {
                Some((c, d)) if d < 120_000.0 => t!("settings.near", crate::tr_en(c)),
                // (to a tenth of a degree: the world map is no finer than that)
                _ => format!("{:.1}°{}  {:.1}°{}", lat.abs(), if lat >= 0.0 { "N" } else { "S" }, lon.abs(), if lon >= 0.0 { "E" } else { "W" }),
            };
            let how = if s.flags & mp_proto::F_GROUND != 0 { t!("settings.on_the_ground").to_string() } else { format!("{:.0} ft  ·  {} kt", s.alt_m() * 3.28084, s.speed) };
            ui.horizontal(|ui| {
                let w = ui.available_width();
                ui.allocate_ui_with_layout(vec2((w - 96.0).max(100.0), 40.0), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(egui::Label::new(RichText::new(format!("{}  ·  {ac}", s.name)).font(menu::label()).color(TEXT)).truncate());
                    ui.add(egui::Label::new(RichText::new(format!("{place}  ·  {how}")).font(menu::caption()).color(DIM)).truncate());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t!("settings.show_2")).on_hover_text(t!("settings.turn_the_globe_to_them")).clicked() {
                        look = Some((lat, lon));
                    }
                });
            });
            ui.add_space(4.0);
        }
        let more = (self.mp_world_total as usize).saturating_sub(self.mp_world.len().min(shown));
        if more > 0 {
            note(ui, &i18n::plural(more as u64, "settings.more"));
        }
        if let Some((lat, lon)) = look {
            self.menu.look_at(lat, lon, 400_000.0);
            self.menu.screen = crate::menu::Screen::Fly;
            self.pad_home = true;
        }
    }

    /// The server: the network's servers with their pilots and ping,
    /// "automatic" (the fastest), or an address typed in.
    fn mp_server_picker(&mut self, ui: &mut Ui) {
        let dir = self.mp_directory();
        let (servers, err, stale) = (dir.servers(), dir.error(), dir.stale());
        let best = dir.best();
        let auto = self.mp_auto();
        let current = mp::with_port(&self.mp_server);
        let listed = servers.iter().any(|s| s.address == current);
        let label = |s: &mp::ServerEntry| {
            let state = match (s.up, s.ping_ms) {
                (false, _) => t!("settings.offline").to_string(),
                (true, Some(ms)) => format!("{ms} ms"),
                (true, None) => t!("settings.no_answer").to_string(),
            };
            format!("{}  ·  {}  ·  {}", s.name, i18n::plural(s.flying as u64, "mp.pilots"), state)
        };
        let selected = if auto {
            match &best {
                Some(b) => t!("settings.automatic_2", b.name),
                None => t!("settings.automatic_the_fastest").to_string(),
            }
        } else if let Some(s) = servers.iter().find(|s| s.address == current) {
            label(s)
        } else {
            t!("settings.other", self.mp_server.trim())
        };
        let mut pick: Option<String> = None;
        egui::ComboBox::from_id_salt("mp_server").selected_text(selected).width(ui.available_width().min(340.0)).show_ui(ui, |ui| {
            if ui.selectable_label(auto, t!("settings.automatic_the_fastest")).clicked() {
                pick = Some("auto".into());
            }
            for s in &servers {
                ui.add_enabled_ui(s.up, |ui| {
                    if ui.selectable_label(!auto && s.address == current, label(s)).clicked() {
                        pick = Some(s.address.clone());
                    }
                });
            }
            if ui.selectable_label(!auto && !listed, t!("settings.other_address")).clicked() {
                pick = Some(String::new());
            }
        });
        match pick.as_deref() {
            Some("") => {
                self.mp_custom = true;
                if auto || listed {
                    self.mp_server.clear();
                }
            }
            Some(a) => {
                self.mp_custom = false;
                self.mp_server = a.to_string();
            }
            None => {}
        }
        if self.mp_custom || (!auto && !listed) {
            ui.add(egui::TextEdit::singleline(&mut self.mp_server).desired_width(ui.available_width().min(340.0)).char_limit(100).hint_text("host[:port]"));
        }
        ui.horizontal(|ui| {
            let text = match (&err, servers.is_empty()) {
                (Some(_), false) if stale => t!("settings.last_known_list_the_list").to_string(),
                (Some(e), _) => t!("settings.server_list_unavailable", e.chars().take(50).collect::<String>()),
                (None, true) => t!("settings.reading_the_server_list").to_string(),
                (None, false) => i18n::plural(servers.iter().filter(|s| s.up).count() as u64, "mp.servers_online"),
            };
            ui.label(RichText::new(text).font(menu::caption()).color(DIM));
            if ui.small_button(t!("settings.refresh")).clicked() {
                self.mp_directory().refresh();
            }
        });
    }

    // ---- joysticks, pedals and keys (bind.rs)

    /// While waiting for an axis, button or key to bind: what to do, and Cancel.
    fn binds_capture_banner(&mut self, ui: &mut Ui) {
        use bind::{Capture, ACTIONS};
        let Some(c) = self.binds.capture.clone() else { return };
        let label = |id: &str| ACTIONS.iter().find(|a| a.id == id).map(|a| a.label()).unwrap_or("?");
        let what = match &c {
            Capture::Axis(a) => a.how().to_string(),
            Capture::Button(id) => t!("settings.press_the_button_for", label(id)),
            Capture::Key(id) => t!("settings.press_the_key_for_esc", label(id)),
            Capture::Pad(id) => t!("settings.press_a_button_for_or", padmap::label(id)),
            Capture::PadAxis(a) => format!("{}: {}", a.label(), a.how()),
        };
        // From the pad: B held calls it off (a tap of B binds B), and how far through that is.
        let from_pad = c.from_pad();
        let cancelling = self.binds.pad_catch.cancelling(self.real);
        egui::Frame::new().fill(ACCENT.gamma_multiply(0.22)).corner_radius(menu::R_PANEL).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(what).font(menu::body()).color(Color32::WHITE));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t!("settings.cancel")).clicked() {
                        self.binds.capture = None;
                    }
                    if from_pad {
                        ui.label(RichText::new(t!("settings.hold_to_cancel")).font(menu::caption()).color(Color32::WHITE));
                        padui::glyphs(ui, "B", 20.0);
                    }
                });
            });
            if cancelling > 0.0 {
                ui.add(egui::ProgressBar::new(cancelling).desired_height(4.0));
            }
        });
        ui.add_space(12.0);
    }

    fn binds_devices(&mut self, ui: &mut Ui) {
        let devices: Vec<(bind::Uuid, String, bool)> = self.gilrs.as_ref().map(|g| g.gamepads().map(|(_, p)| (p.uuid(), p.name().to_string(), self.binds.uses(&p.uuid()))).collect()).unwrap_or_default();
        if devices.is_empty() {
            note(ui, t!("settings.no_joystick_pedals_or_gamepad"));
        }
        for (k, (uuid, name, bound)) in devices.into_iter().enumerate() {
            let role = if bound { t!("settings.bound") } else { t!("settings.gamepad_layout") };
            row(ui, &name, role, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("preset", k)).selected_text(t!("settings.apply_a_preset")).width(260.0).show_ui(ui, |ui| {
                        for p in bind::PRESETS {
                            if ui.selectable_label(false, p.label()).clicked() {
                                self.binds.apply_preset(p, uuid, &name, true);
                            }
                        }
                    });
                    if bound && ui.button(t!("settings.remove_its_bindings")).clicked() {
                        self.binds.clear_device(&uuid);
                    }
                });
            });
        }
        note(ui, t!("settings.a_gamepad_has_its_own"));
    }

    /// The gamepad's flying layout (padmap.rs): the sticks and triggers,
    /// then every action, each bound from the pad itself.
    fn binds_pad(&mut self, ui: &mut Ui, words: &[String]) {
        use bind::Capture;
        use padmap::{AxisSrc, PadAxis, Thumb};
        let busy = self.binds.capture.is_some();
        note(ui, t!("settings.in_the_menus_the_pad"));
        if self.pad_note.1 > self.real {
            ui.add_space(4.0);
            ui.label(RichText::new(&self.pad_note.0).font(menu::caption()).color(ACCENT));
        }
        ui.add_space(8.0);
        let cw = ui.available_width();
        let mut lost = Vec::new();
        bind_grid(ui, "pad_binds", |ui| {
            for a in PadAxis::ALL {
                let src = self.binds.pad.axis(a);
                if !menu::matches(words, &[a.label(), &src.map(|s| s.stick.spec()).unwrap_or_default()]) {
                    continue;
                }
                grid_label(ui, a.label(), cw);
                pad_cell(ui, src.map(|s| s.stick.spec()), self.binds.pad.axis_moved(a));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(t!("settings.change"))).clicked() {
                        self.binds.start_capture(Capture::PadAxis(a));
                    }
                    if src.is_some() && ui.add_enabled(!busy, egui::Button::new(t!("settings.clear"))).clicked() {
                        lost = self.binds.pad.set_axis(a, None);
                    }
                    if self.binds.pad.axis_moved(a) && ui.add_enabled(!busy, egui::Button::new(t!("settings.default"))).clicked() {
                        lost = self.binds.pad.reset_axis(a);
                    }
                    if let Some(s) = src {
                        let mut inv = s.inverted(a);
                        if menu::toggle(ui, &mut inv, t!("settings.invert")).changed() {
                            self.binds.pad.set_axis(a, Some(AxisSrc { sign: -s.sign, ..s }));
                        }
                    }
                });
                ui.end_row();
            }
            // Each thumbstick's dead zone, for a pad that drifts at rest.
            for t in Thumb::ALL {
                let mut d = self.binds.pad.dead(t);
                grid_label(ui, t.label(), cw);
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = 70.0;
                    if ui.add(egui::Slider::new(&mut d, 0.0..=padmap::DEAD_MAX).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)))
                        .on_hover_text(t!("settings.dead_zone_how_much_travel"))
                        .changed()
                    {
                        self.binds.pad.set_dead(t, d);
                    }
                });
                ui.horizontal(|ui| {
                    dead_zone_box(ui, t, d, self.pad_live);
                    if d != t.default_dead() && ui.add_enabled(!busy, egui::Button::new(t!("settings.default"))).clicked() {
                        self.binds.pad.set_dead(t, t.default_dead());
                    }
                });
                ui.end_row();
            }
            // (one grid for both, so the columns line up)
            for _ in 0..3 {
                ui.allocate_space(vec2(0.0, 4.0));
            }
            ui.end_row();
            for &(id, _) in padmap::actions() {
                let moved = self.binds.pad.is_moved(id);
                let chord = self.binds.pad.chord(id);
                if !menu::matches(words, &[padmap::label(id), id, &chord.map(|c| c.spec()).unwrap_or_default()]) {
                    continue;
                }
                grid_label(ui, padmap::label(id), cw);
                pad_cell(ui, chord.map(|c| if c == padmap::Y_ALONE { t!("settings.y_let_go").to_string() } else { c.spec() }), moved);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(t!("settings.change"))).clicked() {
                        self.binds.start_capture(Capture::Pad(id));
                    }
                    if chord.is_some() && ui.add_enabled(!busy, egui::Button::new(t!("settings.clear"))).clicked() {
                        self.binds.pad.clear(id);
                    }
                    if moved && ui.add_enabled(!busy, egui::Button::new(t!("settings.default"))).clicked() {
                        lost = self.binds.pad.reset(id);
                    }
                });
                ui.end_row();
            }
        });
        if !lost.is_empty() {
            self.pad_note = (t!("settings.no_longer_bound", lost.join(", ")), self.real + 8.0);
        }
        ui.add_space(8.0);
        if ui.add_enabled(!busy && !self.binds.pad.is_default(), egui::Button::new(t!("settings.back_to_the_flight_simulator"))).clicked() {
            self.binds.pad = padmap::PadMap::default();
            self.pad_note = (String::new(), 0.0);
        }
    }

    fn binds_axes(&mut self, ui: &mut Ui, words: &[String]) {
        use bind::{AxisAct, Capture};
        let busy = self.binds.capture.is_some();
        note(ui, t!("settings.binding_then_its_live_value"));
        ui.add_space(6.0);
        let cw = ui.available_width();
        bind_grid(ui, "bind_axes", |ui| {
            let shown: Vec<_> = AxisAct::ALL.into_iter().filter(|a| menu::matches(words, &[a.label(), &self.binds.describe_axis(*a).unwrap_or_default()])).collect();
            for a in shown {
                grid_label(ui, a.label(), cw);
                binding_cell(ui, self.binds.describe_axis(a), false);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(t!("settings.assign"))).clicked() {
                        self.binds.start_capture(Capture::Axis(a));
                    }
                    if self.binds.axes.contains_key(&a) && ui.button(t!("settings.clear")).clicked() {
                        self.binds.axes.remove(&a);
                    }
                });
                ui.horizontal(|ui| {
                    // 0 until the axis moves.
                    if let Some(v) = self.binds.axis(a).or_else(|| self.binds.axes.contains_key(&a).then_some(0.0)) {
                        let f = if a.centred() { (v as f32 + 1.0) * 0.5 } else { v as f32 };
                        ui.add(egui::ProgressBar::new(f).desired_width(64.0).desired_height(6.0));
                    }
                    // the dead zone.
                    if let Some(b) = self.binds.axes.get_mut(&a) {
                        ui.spacing_mut().slider_width = 70.0;
                        ui.add(egui::Slider::new(&mut b.dead, 0.0..=0.3).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)))
                            .on_hover_text(t!("settings.dead_zone_how_much_travel"));
                        // The sign is learnt when binding; this flips it.
                        let mut inv = b.sign < 0.0;
                        if menu::toggle(ui, &mut inv, t!("settings.invert")).changed() {
                            b.sign = -b.sign;
                        }
                    }
                });
                ui.end_row();
            }
        });
    }

    fn binds_buttons(&mut self, ui: &mut Ui, words: &[String]) {
        use bind::Capture;
        let busy = self.binds.capture.is_some();
        let cw = ui.available_width();
        let ctx = self.bind_ctx;
        let shown: Vec<_> = bind::actions_in(ctx).filter(|a| menu::matches(words, &[a.label(), a.id, &self.binds.describe_button(a.id).unwrap_or_default()])).collect();
        bind_grid(ui, "bind_buttons", |ui| {
            for a in shown {
                grid_label(ui, a.label(), cw);
                binding_cell(ui, self.binds.describe_button(a.id), false);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(t!("settings.assign"))).clicked() {
                        self.binds.start_capture(Capture::Button(a.id));
                    }
                    if self.binds.buttons.contains_key(a.id) && ui.button(t!("settings.clear")).clicked() {
                        self.binds.buttons.remove(a.id);
                    }
                });
                ui.label("");
                ui.end_row();
            }
        });
    }

    fn binds_keys(&mut self, ui: &mut Ui, words: &[String]) {
        use bind::Capture;
        let busy = self.binds.capture.is_some();
        let cw = ui.available_width();
        let ctx = self.bind_ctx;
        let shown: Vec<_> = bind::actions_in(ctx).filter(|a| a.keyboard).collect();
        bind_grid(ui, "bind_keys", |ui| {
            for a in shown {
                let moved = self.binds.keys.contains_key(a.id);
                let keys = if a.id == "tail_view" && !moved && self.binds.map_key(ctx, winit::keyboard::KeyCode::IntlBackslash) == Some(a.key) {
                    Some(i18n::tr("text.controls.f6_iso_key").to_string())
                } else {
                    self.binds.key_of(ctx, a).map(bind::key_name)
                };
                if !menu::matches(words, &[a.label(), a.id, keys.as_deref().unwrap_or_default()]) {
                    continue;
                }
                grid_label(ui, a.label(), cw);
                binding_cell(ui, keys, moved);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(t!("settings.change"))).clicked() {
                        self.binds.start_capture(Capture::Key(a.id));
                        self.keys.clear();
                    }
                    if moved && ui.button(t!("settings.default")).clicked() {
                        self.binds.reset_key(ctx, a.id);
                        self.keys.clear();
                    }
                });
                ui.label("");
                ui.end_row();
            }
        });
    }
}

/// the grid every bindings table shares, so their columns line up.
/// Where a thumbstick (or the triggers) is now, against its dead zone: a
/// box with the zone shaded and a dot for the stick, lit once it gets
/// through; the triggers as two bars. Then how far off centre it reads, to
/// see what a pad at rest drifts to.
fn dead_zone_box(ui: &mut Ui, t: padmap::Thumb, d: f32, live: Option<padmap::Live>) {
    let side = 56.0;
    let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let p = ui.painter();
    let zone = ACCENT.gamma_multiply(0.22);
    p.rect_filled(r, 3.0, Color32::from_black_alpha(90));
    let v = live.map(|l| l.at(t)).unwrap_or_default();
    let lit = live.map(|l| l.through(t, d)).unwrap_or_default();
    if t == padmap::Thumb::Triggers {
        // LT and RT, filling up from the bottom; the zone along the bottom.
        let gap = 6.0;
        let w = (side - gap * 3.0) * 0.5;
        for (i, (v, lit)) in v.into_iter().zip(lit).enumerate() {
            let x = r.left() + gap + i as f32 * (w + gap);
            let bar = Rect::from_min_max(pos2(x, r.top() + 4.0), pos2(x + w, r.bottom() - 4.0));
            p.rect_stroke(bar, 1.0, Stroke::new(1.0, DIM), egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(pos2(bar.left(), bar.bottom() - bar.height() * d), bar.max), 0.0, zone);
            if live.is_some() {
                let col = if lit { ACCENT } else { TEXT };
                p.rect_filled(Rect::from_min_max(pos2(bar.left(), bar.bottom() - bar.height() * v.clamp(0.0, 1.0)), bar.max), 0.0, col.gamma_multiply(0.8));
            }
        }
    } else {
        // Each way reads nothing inside its band: a cross, shaded.
        let (c, h) = (r.center(), side * 0.5);
        p.rect_filled(Rect::from_center_size(c, vec2(side * d, side)), 0.0, zone);
        p.rect_filled(Rect::from_center_size(c, vec2(side, side * d)), 0.0, zone);
        p.line_segment([pos2(c.x, r.top()), pos2(c.x, r.bottom())], Stroke::new(1.0, DIM.gamma_multiply(0.5)));
        p.line_segment([pos2(r.left(), c.y), pos2(r.right(), c.y)], Stroke::new(1.0, DIM.gamma_multiply(0.5)));
        if live.is_some() {
            let dot = pos2(c.x + v[0].clamp(-1.0, 1.0) * h, c.y - v[1].clamp(-1.0, 1.0) * h);
            p.circle_filled(dot, 3.5, if lit[0] || lit[1] { ACCENT } else { TEXT });
        }
    }
    p.rect_stroke(r, 3.0, Stroke::new(1.0, DIM), egui::StrokeKind::Inside);
    // The reading, in the slider's terms.
    let text = match live {
        Some(_) => {
            let (a, b) = if t == padmap::Thumb::Triggers { ("LT", "RT") } else { ("X", "Y") };
            format!("{a} {:.0}%  {b} {:.0}%", v[0].abs() * 100.0, v[1].abs() * 100.0)
        }
        None => t!("settings.no_gamepad").to_string(),
    };
    ui.add_sized(vec2(96.0, side), egui::Label::new(RichText::new(text).font(menu::caption()).color(DIM)));
}

fn bind_grid(ui: &mut Ui, id: &str, body: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(4).spacing([16.0, 6.0]).min_row_height(30.0).show(ui, body);
}

/// what an action is bound to, as a key cap.
fn binding_cell(ui: &mut Ui, what: Option<String>, moved: bool) {
    ui.allocate_ui_with_layout(vec2(180.0, 26.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(180.0);
        match what {
            Some(w) => {
                let r = menu::key_cap(ui, &w);
                if moved {
                    ui.painter().rect_stroke(r.rect, menu::R_WIDGET, Stroke::new(1.0, ACCENT), egui::StrokeKind::Outside);
                }
            }
            None => {
                ui.label(RichText::new(t!("settings.not_bound")).font(menu::caption()).color(menu::FAINT));
            }
        }
    });
}

/// What a pad action is bound to, drawn as the pad's buttons, in the same
/// column as `binding_cell`. `moved`: changed from the default.
fn pad_cell(ui: &mut Ui, spec: Option<String>, moved: bool) {
    ui.allocate_ui_with_layout(vec2(180.0, 26.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(180.0);
        match spec {
            Some(s) => {
                let r = padui::glyphs(ui, &s, 22.0);
                if moved {
                    ui.painter().rect_stroke(r.rect.expand(3.0), menu::R_WIDGET, Stroke::new(1.0, ACCENT), egui::StrokeKind::Outside);
                }
            }
            None => {
                ui.label(RichText::new(t!("settings.not_bound")).font(menu::caption()).color(menu::FAINT));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_parse_and_cycle() {
        for s in Section::ALL {
            assert_eq!(Section::parse(s.name()), Some(s));
            assert_eq!(s.cycle(true).cycle(false), s);
        }
        assert_eq!(Section::parse("bindings"), Some(Section::Controls));
        assert_eq!(Section::parse("Graphics"), Some(Section::Graphics));
        assert_eq!(Section::Map.cycle(true), Section::Multiplayer);
        assert_eq!(Section::Multiplayer.cycle(true), Section::General);
        assert_eq!(Section::parse("mp"), Some(Section::Multiplayer));
        assert_eq!(Section::parse("nope"), None);
        // `--pause-menu controls` is the key list, not the settings section.
        assert_eq!(crate::menu::Page::parse("controls"), Some(crate::menu::Page::Controls));
        assert_eq!(crate::menu::Page::parse("graphics"), None);
    }

    #[test]
    fn controls_ui_does_not_panic() {
        let ctx = egui::Context::default();
        let binds = bind::Bindings::default();
        let mut look_sens = 1.0f32;
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        card(ui, t!("settings.mouse"), |ui| {
                            row(ui, t!("settings.mouse_look_speed"), t!("settings.dragging_to_look_around"), |ui| {
                                ui.add(egui::Slider::new(&mut look_sens, 0.25..=3.0).logarithmic(true).custom_formatter(|v, _| format!("{v:.2}×")));
                            });
                        });
                        let _words = menu::search_box(ui, "bindings", t!("menu.search_bindings"), ui.available_width().min(460.0));
                        ui.add_space(6.0);
                        card(ui, t!("settings.gamepad"), |ui| {
                            let cw = ui.available_width();
                            bind_grid(ui, "pad_binds", |ui| {
                                for a in padmap::PadAxis::ALL {
                                    let src = binds.pad.axis(a);
                                    grid_label(ui, a.label(), cw);
                                    pad_cell(ui, src.map(|s| s.stick.spec()), binds.pad.axis_moved(a));
                                    ui.horizontal(|ui| {
                                        let _ = ui.button(t!("settings.change"));
                                    });
                                    ui.end_row();
                                }
                                for _ in 0..3 {
                                    ui.allocate_space(vec2(0.0, 4.0));
                                }
                                ui.end_row();
                                for &(id, _) in padmap::actions() {
                                    let moved = binds.pad.is_moved(id);
                                    let chord = binds.pad.chord(id);
                                    grid_label(ui, padmap::label(id), cw);
                                    pad_cell(ui, chord.map(|c| if c == padmap::Y_ALONE { t!("settings.y_let_go").to_string() } else { c.spec() }), moved);
                                    ui.horizontal(|ui| {
                                        let _ = ui.button(t!("settings.change"));
                                    });
                                    ui.end_row();
                                }
                            });
                        });
                        card(ui, t!("settings.joystick_axes"), |ui| {
                            let cw = ui.available_width();
                            bind_grid(ui, "bind_axes", |ui| {
                                for a in bind::AxisAct::ALL {
                                    grid_label(ui, a.label(), cw);
                                    binding_cell(ui, binds.describe_axis(a), false);
                                    ui.horizontal(|ui| {
                                        let _ = ui.button(t!("settings.assign"));
                                    });
                                    ui.horizontal(|_ui| {});
                                    ui.end_row();
                                }
                            });
                        });
                        card(ui, t!("settings.buttons_joystick_throttle_wheel"), |ui| {
                            let cw = ui.available_width();
                            bind_grid(ui, "bind_buttons", |ui| {
                                for a in bind::ACTIONS {
                                    grid_label(ui, a.label(), cw);
                                    binding_cell(ui, binds.describe_button(a.id), false);
                                    ui.horizontal(|ui| {
                                        let _ = ui.button(t!("settings.assign"));
                                    });
                                    ui.label("");
                                    ui.end_row();
                                }
                            });
                        });
                        card(ui, t!("settings.keyboard"), |ui| {
                            let cw = ui.available_width();
                            bind_grid(ui, "bind_keys", |ui| {
                                for a in bind::ACTIONS.iter().filter(|a| a.keyboard) {
                                    grid_label(ui, a.label(), cw);
                                    let keys = binds.key_of(bind::Context::Flight, a).map(bind::key_name);
                                    binding_cell(ui, keys, false);
                                    ui.horizontal(|ui| {
                                        let _ = ui.button(t!("settings.change"));
                                    });
                                    ui.label("");
                                    ui.end_row();
                                }
                            });
                        });
                        card(ui, t!("settings.key_reference"), |ui| {
                            menu::controls_list(ui);
                        });
                    });
                });
            });
            out.textures_delta.clear();
        }
    }
}
