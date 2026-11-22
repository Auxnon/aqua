//! The desktop UI, laid out like the Android and web apps: navigation and
//! history top left, actions top right (what doesn't fit goes in ⋮), size and
//! wetness on the left rail, tools in the bottom dock, panels over the canvas.

use std::time::{Duration, SystemTime};

use aqua_engine::brush::Tool;
use aqua_engine::pigment::{self, Pigment};
use egui::{Align2, Area, Color32, Context, CornerRadius, Frame, Id, Key, Margin, Modal, Order, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::icons as ic;
use crate::studio::{BrushP, Dialog, PAPER_BLACK, PAPER_BRIGHT, PAPER_WHITE, Panel, Screen, Studio, rail_to_size, size_to_rail};
use crate::sync::PushState;

pub struct Theme {
    pub bg: Color32,
    pub panel: Color32,
    pub panel_solid: Color32,
    pub raised: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub danger: Color32,
}

const THEME: Theme = Theme {
    bg: Color32::from_rgb(9, 9, 11),
    panel: Color32::from_rgba_premultiplied(23, 23, 27, 230),
    panel_solid: Color32::from_rgb(26, 26, 30),
    raised: Color32::from_rgb(42, 42, 48),
    text: Color32::from_rgb(236, 234, 230),
    text_dim: Color32::from_rgb(154, 152, 148),
    accent: Color32::from_rgb(143, 211, 203),
    accent_soft: Color32::from_rgba_premultiplied(31, 46, 45, 56),
    danger: Color32::from_rgb(229, 128, 111),
};

impl Theme {
    pub fn get() -> &'static Theme {
        &THEME
    }
}

pub fn style(ctx: &Context) {
    let t = Theme::get();
    let mut v = egui::Visuals::dark();
    v.panel_fill = t.panel_solid;
    v.window_fill = t.panel_solid;
    v.extreme_bg_color = t.raised;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent.gamma_multiply(0.5);
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.widgets.inactive.bg_fill = t.raised;
    v.widgets.inactive.weak_bg_fill = t.raised;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 52, 60);
    v.widgets.active.weak_bg_fill = t.accent.gamma_multiply(0.4);
    v.slider_trailing_fill = true;
    v.window_corner_radius = CornerRadius::same(20);
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.slider_width = 170.0;
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 5.0);
    });
}

fn panel_frame() -> Frame {
    Frame::new().fill(THEME.panel).corner_radius(CornerRadius::same(24)).inner_margin(Margin::symmetric(4, 0))
}

fn card_frame() -> Frame {
    Frame::new().fill(THEME.panel).corner_radius(CornerRadius::same(18)).inner_margin(Margin::same(14))
}

/// How a pigment looks on white paper, and at full strength.
fn swatch_colors(p: Pigment) -> (Color32, Color32) {
    let s = pigment::strength(p);
    let mass = if s >= 0.999 { p } else { p.map(|v| v / s.max(1e-4)) };
    (srgb(pigment::to_srgb(p)), srgb(pigment::to_srgb(mass)))
}

fn srgb(c: [f32; 3]) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

/// A round swatch; thinned paint shows its pale tint ringed with the full-strength colour.
fn swatch(ui: &mut Ui, p: Pigment, size: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let (tint, mass) = swatch_colors(p);
    let pt = ui.painter();
    let r = size / 2.0;
    pt.circle_filled(rect.center(), r, tint);
    if pigment::strength(p) < 0.97 {
        pt.circle_stroke(rect.center(), r - 1.5, Stroke::new(2.5, mass));
    }
    pt.circle_stroke(rect.center(), r, Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { THEME.text } else { Color32::from_white_alpha(50) }));
    resp
}

pub fn draw(root: &mut Ui, st: &mut Studio) {
    let ctx = &root.ctx().clone();
    match st.screen {
        Screen::Gallery => gallery(root, st),
        Screen::Canvas => canvas(ctx, st),
    }
    dialogs(ctx, st);
    if let Some((msg, at)) = st.toast.clone() {
        let age = at.elapsed().as_secs_f32();
        if age > 2.2 {
            st.toast = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
            let alpha = (2.2 - age).clamp(0.0, 0.4) / 0.4;
            Area::new(Id::unique("toast")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -96.0)).order(Order::Tooltip).interactable(false).show(ctx, |ui| {
                Frame::new().fill(THEME.panel.gamma_multiply(alpha)).corner_radius(CornerRadius::same(14)).inner_margin(Margin::symmetric(14, 7)).show(ui, |ui| {
                    ui.label(RichText::new(msg).color(THEME.text.gamma_multiply(alpha)));
                });
            });
        }
    }
}

// ---------------------------------------------------------------- canvas

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Dip,
    Sync,
    Export,
    Hide,
}

const ACTIONS: [Action; 4] = [Action::Dip, Action::Sync, Action::Export, Action::Hide];

fn action_look(st: &Studio, a: Action) -> (&'static [&'static str], String, bool, Option<Color32>) {
    match a {
        Action::Dip => {
            let dip = st.settings.dip;
            (if dip { ic::CUP } else { ic::ENDLESS }, if dip { "Brush runs dry: dip it in the cup".into() } else { "Brush always wet".into() }, dip, None)
        }
        Action::Sync => {
            let s = st.sync.status();
            let on = st.settings.sync.on;
            let tint = if !on {
                None
            } else if matches!(s.primary, PushState::Failed(_)) && !matches!(s.backup, PushState::Done(_)) {
                Some(THEME.danger)
            } else {
                Some(THEME.accent)
            };
            (ic::SYNC, if on { "Sync: on".into() } else { "Sync".into() }, false, tint)
        }
        Action::Export => (ic::SHARE, "Export PNG".into(), false, None),
        Action::Hide => (ic::EYE_OFF, "Hide controls (H)".into(), false, None),
    }
}

fn run_action(st: &mut Studio, a: Action) {
    match a {
        Action::Dip => {
            let on = !st.settings.dip;
            st.set_dip(on, true);
        }
        Action::Sync => {
            st.panel = Panel::None;
            st.dialog = Dialog::Sync { test: String::new() };
        }
        Action::Export => st.export(),
        Action::Hide => {
            st.panel = Panel::None;
            st.hidden = true;
        }
    }
}

fn canvas(ctx: &Context, st: &mut Studio) {
    shortcuts(ctx, st);
    if st.hidden {
        Area::new(Id::unique("unhide")).anchor(Align2::RIGHT_TOP, vec2(-8.0, 8.0)).show(ctx, |ui| {
            Frame::new().fill(THEME.panel.gamma_multiply(0.5)).corner_radius(CornerRadius::same(22)).show(ui, |ui| {
                if ic::button(ui, ic::EYE, 44.0, false, true, Some(THEME.text.gamma_multiply(0.7)), "Show controls (H)").clicked() {
                    st.hidden = false;
                }
            });
        });
        return;
    }
    let width = ctx.content_rect().width();
    let btn = if width < 600.0 { 40.0 } else { 44.0 };
    let info = st.engine.canvas.as_ref().map(|c| (c.can_undo(), c.can_redo(), c.layers.get(c.active).is_some_and(|l| l.wet.is_some())));
    let (can_undo, can_redo, wet) = info.unwrap_or_default();

    Area::new(Id::unique("bar-left")).anchor(Align2::LEFT_TOP, vec2(8.0, 6.0)).show(ctx, |ui| {
        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if ic::button(ui, ic::BACK, btn, false, true, None, "Gallery").clicked() {
                    st.leave();
                }
                if ic::button(ui, ic::UNDO, btn, false, can_undo, None, "Undo (Ctrl+Z)").clicked() {
                    st.undo();
                }
                if ic::button(ui, ic::REDO, btn, false, can_redo, None, "Redo (Ctrl+Shift+Z)").clicked() {
                    st.redo();
                }
            });
        });
    });

    // Back, undo, redo; dry, layers, paint and ⋮; and the bars' padding.
    let slots = ((width - 40.0 - btn * 7.0) / btn).floor().max(0.0) as usize;
    let inline = if slots >= ACTIONS.len() { ACTIONS.len() } else { slots };
    let mut more_rect = None;
    Area::new(Id::unique("bar-right")).anchor(Align2::RIGHT_TOP, vec2(-8.0, 6.0)).show(ctx, |ui| {
        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if wet
                    && ic::button(ui, ic::DRY, btn, false, true, None, "Dry layer now").clicked()
                    && let Some(c) = st.engine.canvas.as_mut()
                {
                    {
                        let a = c.active;
                        c.dry_layer(a);
                        st.engine.needs_present = true;
                    }
                }
                for &a in &ACTIONS[..inline] {
                    let (icon, label, sel, tint) = action_look(st, a);
                    if ic::button(ui, icon, btn, sel, true, tint, &label).clicked() {
                        run_action(st, a);
                    }
                }
                if ic::button(ui, ic::LAYERS, btn, st.panel == Panel::Layers || st.panel == Panel::Paper, true, None, "Layers").clicked() {
                    st.panel = if matches!(st.panel, Panel::Layers | Panel::Paper) { Panel::None } else { Panel::Layers };
                }
                ui.add_space((btn - 32.0) / 2.0);
                if swatch(ui, st.color(), 32.0, st.panel == Panel::Palette).on_hover_text("Palette").clicked() {
                    st.panel = if st.panel == Panel::Palette { Panel::None } else { Panel::Palette };
                }
                ui.add_space((btn - 32.0) / 2.0);
                if inline < ACTIONS.len() {
                    let r = ic::button(ui, ic::MORE, btn, st.more_open, true, None, "More");
                    if r.clicked() {
                        st.more_open = !st.more_open;
                    }
                    more_rect = Some(r.rect);
                }
            });
        });
    });
    if let (true, Some(r)) = (st.more_open, more_rect) {
        let resp = Area::new(Id::unique("more")).fixed_pos(pos2(r.right() - 220.0, r.bottom() + 8.0)).order(Order::Foreground).show(ctx, |ui| {
            Frame::new().fill(THEME.panel_solid).corner_radius(CornerRadius::same(16)).inner_margin(Margin::symmetric(4, 6)).show(ui, |ui| {
                ui.set_width(212.0);
                for &a in &ACTIONS[inline..] {
                    let (icon, label, sel, tint) = action_look(st, a);
                    let (rect, row) = ui.allocate_exact_size(vec2(212.0, 42.0), Sense::click());
                    if row.hovered() {
                        ui.painter().rect_filled(rect, 10.0, Color32::from_white_alpha(14));
                    }
                    let col = tint.unwrap_or(if sel { THEME.accent } else { THEME.text });
                    ic::paint(ui, ic::center_rect(rect.left_center() + vec2(26.0, 0.0), 20.0), icon, col);
                    let label = label.trim_end_matches(" (H)").to_string();
                    ui.painter().text(rect.left_center() + vec2(52.0, 0.0), Align2::LEFT_CENTER, label, egui::FontId::proportional(15.0), if sel { THEME.accent } else { THEME.text });
                    if row.clicked() {
                        st.more_open = false;
                        run_action(st, a);
                    }
                }
            });
        });
        if ctx.input(|i| i.pointer.any_pressed()) && !resp.response.contains_pointer() && !r.contains(ctx.input(|i| i.pointer.interact_pos().unwrap_or_default())) {
            st.more_open = false;
        }
    }

    rail(ctx, st);
    dock(ctx, st);
    if st.settings.dip {
        cup(ctx, st);
    }
    match st.panel {
        Panel::Layers => layers_panel(ctx, st),
        Panel::Paper => paper_panel(ctx, st),
        Panel::Palette => palette_panel(ctx, st),
        Panel::Brush => brush_panel(ctx, st),
        Panel::Picker => picker_panel(ctx, st),
        Panel::None => {}
    }
}

fn shortcuts(ctx: &Context, st: &mut Studio) {
    if ctx.egui_wants_keyboard_input() || !matches!(st.dialog, Dialog::None) {
        return;
    }
    let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
    let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
    if cmd && pressed(Key::Z) {
        if shift { st.redo() } else { st.undo() }
    } else if cmd && pressed(Key::Y) {
        st.redo();
    } else if cmd && pressed(Key::S) {
        st.save(true);
        st.say("Saved");
    } else if cmd {
    } else if pressed(Key::H) {
        st.hidden = !st.hidden;
        st.panel = Panel::None;
    } else if pressed(Key::Escape) {
        if st.hidden {
            st.hidden = false;
        } else {
            st.panel = Panel::None;
            st.more_open = false;
        }
    } else if pressed(Key::OpenBracket) || pressed(Key::CloseBracket) {
        let mut p = st.bp();
        p.size = rail_to_size(size_to_rail(p.size) + if pressed(Key::CloseBracket) { 0.04 } else { -0.04 });
        st.set_bp(p);
    } else if pressed(Key::B) {
        st.select_tool(Some(Tool::Watercolor));
    } else if pressed(Key::W) && !st.settings.dip {
        st.select_tool(Some(Tool::Water));
    } else if pressed(Key::I) {
        st.select_tool(Some(Tool::Ink));
    } else if pressed(Key::E) {
        st.select_tool(Some(Tool::Eraser));
    } else if pressed(Key::P) && !st.settings.dip {
        st.select_tool(Some(Tool::Push));
    } else if pressed(Key::D) && st.settings.dip {
        st.dip_brush();
    } else if pressed(Key::K) {
        st.select_tool(None);
    } else if pressed(Key::Num0) {
        st.fit();
    } else if pressed(Key::Equals) || pressed(Key::Plus) {
        let (w, h) = st.window;
        st.zoom_at(1.25, w / 2.0, h / 2.0);
    } else if pressed(Key::Minus) {
        let (w, h) = st.window;
        st.zoom_at(0.8, w / 2.0, h / 2.0);
    }
}

fn rail(ctx: &Context, st: &mut Studio) {
    Area::new(Id::unique("rail")).anchor(Align2::LEFT_CENTER, vec2(6.0, 0.0)).show(ctx, |ui| {
        Frame::new().fill(THEME.panel).corner_radius(CornerRadius::same(16)).inner_margin(Margin::symmetric(6, 10)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().slider_width = 170.0;
                let mut p = st.bp();
                let mut v = size_to_rail(p.size);
                if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).vertical().show_value(false)).changed() {
                    p.size = rail_to_size(v);
                }
                ui.label(RichText::new(format!("{}", p.size.round() as i32)).size(12.0).color(THEME.text_dim));
                ui.add_space(10.0);
                let (field, label): (Option<&mut f32>, &str) = match st.brush_tool {
                    Tool::Watercolor | Tool::Water => (Some(&mut p.water), "water"),
                    Tool::Ink => (Some(&mut p.bleed), "bleed"),
                    Tool::Eraser | Tool::Push => (Some(&mut p.strength), "strength"),
                };
                if let Some(f) = field {
                    ui.spacing_mut().slider_width = 120.0;
                    ui.add(egui::Slider::new(f, 0.0..=1.0).vertical().show_value(false));
                    ui.label(RichText::new(label).size(12.0).color(THEME.text_dim));
                }
                if p != st.bp() {
                    st.set_bp(p);
                }
            });
        });
    });
}

fn dock(ctx: &Context, st: &mut Studio) {
    let tools: [(Option<Tool>, &[&str], &str); 6] = [
        (Some(Tool::Watercolor), ic::BRUSH, "Watercolour (B)"),
        (Some(Tool::Water), ic::DROP, "Clear water (W)"),
        (Some(Tool::Ink), ic::QUILL, "Ink (I)"),
        (Some(Tool::Eraser), ic::ERASER, "Eraser (E)"),
        (Some(Tool::Push), ic::PUSH, "Push (P)"),
        (None, if st.settings.pick_wash { ic::WASH_PICK } else { ic::DROPPER }, "Eyedropper (K)"),
    ];
    Area::new(Id::unique("dock")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -12.0)).show(ctx, |ui| {
        Frame::new().fill(THEME.panel).corner_radius(CornerRadius::same(28)).inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (t, icon, label) in tools {
                    if st.settings.dip && matches!(t, Some(Tool::Water | Tool::Push)) {
                        continue;
                    }
                    if ic::button(ui, icon, 48.0, st.tool == t, true, None, label).clicked() {
                        st.select_tool(t);
                    }
                }
            });
        });
    });
}

/// The water cup seen from above, or the ink well with the pen; the ring is what the brush still holds.
fn cup(ctx: &Context, st: &mut Studio) {
    let well = st.brush_tool == Tool::Ink;
    let charge = st.charge().clamp(0.0, 1.0);
    Area::new(Id::unique("cup")).anchor(Align2::LEFT_BOTTOM, vec2(12.0, -12.0)).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
        let resp = resp.on_hover_text(if well { "Ink well: dip the pen (D)" } else { "Water cup: dip the brush (D)" });
        if resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.any_pressed()) {
            st.dip_brush();
        }
        let p = ui.painter();
        let (c, r) = (rect.center(), 30.0);
        p.circle_filled(c + vec2(0.0, 1.5), r, Color32::from_black_alpha(46));
        p.circle_filled(c, r, THEME.panel_solid);
        if well {
            p.circle_filled(c, r * 0.86, Color32::from_rgb(58, 62, 68));
            p.circle_filled(c, r * 0.6, Color32::from_black_alpha(150));
            let (_, mass) = swatch_colors(st.ink);
            p.circle_filled(c, r * 0.52, mass);
            p.circle_filled(c - vec2(r * 0.2, r * 0.22), r * 0.09, Color32::from_white_alpha(110));
        } else {
            p.circle_filled(c, r * 0.86, Color32::from_white_alpha(217));
            p.circle_filled(c, r * 0.78, Color32::from_rgb(170, 207, 233));
            p.circle_filled(c - vec2(r * 0.3, r * 0.32), r * 0.16, Color32::from_white_alpha(140));
        }
        let ring = r - 3.6;
        p.circle_stroke(c, ring, Stroke::new(3.6, THEME.text_dim.gamma_multiply(0.25)));
        if charge > 0.005 {
            let col = if charge < 0.15 { Color32::from_rgb(208, 128, 64) } else { THEME.accent };
            let n = (64.0 * charge).ceil() as usize + 1;
            let pts: Vec<_> = (0..n)
                .map(|k| {
                    let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * charge * k as f32 / (n - 1) as f32;
                    c + vec2(a.cos(), a.sin()) * ring
                })
                .collect();
            p.line(pts, Stroke::new(3.6, col));
        }
    });
}

fn side_panel(ctx: &Context, id: &str, add: impl FnOnce(&mut Ui)) {
    Area::new(Id::unique(id)).anchor(Align2::RIGHT_TOP, vec2(-10.0, 60.0)).order(Order::Foreground).show(ctx, |ui| {
        card_frame().show(ui, |ui| {
            ui.set_width(280.0);
            add(ui);
        });
    });
}

fn bottom_panel(ctx: &Context, id: &str, add: impl FnOnce(&mut Ui)) {
    Area::new(Id::unique(id)).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -80.0)).order(Order::Foreground).show(ctx, |ui| {
        card_frame().show(ui, |ui| add(ui));
    });
}

fn layers_panel(ctx: &Context, st: &mut Studio) {
    let Some(c) = st.engine.canvas.as_mut() else { return };
    let infos = c.layer_infos();
    let active = c.active;
    let paper = c.paper_color();
    let mut open_paper = false;
    let mut changed = false;
    side_panel(ctx, "layers", |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Layers").size(17.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ic::button(ui, ic::ADD, 34.0, false, true, None, "Add layer").clicked() {
                    c.add_layer();
                    changed = true;
                }
            });
        });
        ui.add_space(4.0);
        for (i, l) in infos.iter().enumerate().rev() {
            let sel = i == active;
            Frame::new().fill(if sel { THEME.accent_soft } else { Color32::TRANSPARENT }).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(4, 2)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ic::button(ui, if l.visible { ic::EYE } else { ic::EYE_OFF }, 30.0, false, true, None, "Show or hide").clicked() {
                        c.set_layer_props(i, l.opacity, !l.visible, None);
                        changed = true;
                    }
                    let name = if l.wet { format!("{}  · wet", l.name) } else { l.name.clone() };
                    let r = ui.add(egui::Label::new(RichText::new(name).color(if sel { THEME.accent } else { THEME.text })).sense(Sense::click()));
                    if r.clicked() {
                        c.set_active(i);
                        changed = true;
                    }
                    ui.allocate_space(vec2(ui.available_width(), 1.0));
                });
            });
        }
        // The paper: the bottom of the stack, and its colour.
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        if r.hovered() {
            ui.painter().rect_filled(rect, 10.0, Color32::from_white_alpha(10));
        }
        ui.painter().circle_filled(rect.left_center() + vec2(19.0, 0.0), 10.0, srgb(paper));
        ui.painter().circle_stroke(rect.left_center() + vec2(19.0, 0.0), 10.0, Stroke::new(1.0, Color32::from_white_alpha(50)));
        ui.painter().text(rect.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, "Paper", egui::FontId::proportional(14.0), THEME.text);
        if r.clicked() {
            open_paper = true;
        }
        if let Some(l) = infos.get(active) {
            ui.separator();
            let mut o = l.opacity;
            ui.label(RichText::new("Opacity").size(12.0).color(THEME.text_dim));
            if ui.add(egui::Slider::new(&mut o, 0.0..=1.0).show_value(false)).changed() {
                c.set_layer_props(active, o, l.visible, None);
                changed = true;
            }
            let mut d = l.drying;
            ui.label(RichText::new("Drying speed").size(12.0).color(THEME.text_dim));
            if ui.add(egui::Slider::new(&mut d, 0.0..=4.0).show_value(false)).changed() {
                c.set_layer_drying(active, d);
                changed = true;
            }
            let mut f = l.flow;
            ui.label(RichText::new("Flow").size(12.0).color(THEME.text_dim));
            if ui.add(egui::Slider::new(&mut f, 0.0..=1.0).show_value(false)).changed() {
                c.set_layer_flow(active, f);
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if ic::button(ui, ic::UP, 36.0, false, active + 1 < infos.len(), None, "Move up").clicked() {
                    c.move_layer(active, active + 1);
                    changed = true;
                }
                if ic::button(ui, ic::DOWN, 36.0, false, active > 0, None, "Move down").clicked() {
                    c.move_layer(active, active - 1);
                    changed = true;
                }
                if ic::button(ui, ic::DRY, 36.0, false, l.wet, None, "Dry now").clicked() {
                    c.dry_layer(active);
                    changed = true;
                }
                if ic::button(ui, ic::CLEAR_LAYER, 36.0, false, true, None, "Clear layer").clicked() {
                    c.clear_layer(active);
                    changed = true;
                }
                if ic::button(ui, ic::TRASH, 36.0, false, infos.len() > 1, Some(THEME.danger), "Delete layer").clicked() {
                    c.remove_layer(active);
                    changed = true;
                }
            });
        }
    });
    if changed {
        st.engine.needs_present = true;
    }
    if open_paper {
        st.panel = Panel::Paper;
    }
}

fn paper_panel(ctx: &Context, st: &mut Studio) {
    let Some(c) = st.engine.canvas.as_ref() else { return };
    let mut rgb = c.paper_color();
    let before = rgb;
    let mut relief = c.paper_relief();
    let relief_before = relief;
    let mut back = false;
    side_panel(ctx, "paper", |ui| {
        ui.horizontal(|ui| {
            if ic::button(ui, ic::BACK, 32.0, false, true, None, "Layers").clicked() {
                back = true;
            }
            ui.label(RichText::new("Paper").size(17.0));
        });
        ui.add_space(6.0);
        let presets = [PAPER_WHITE, PAPER_BRIGHT, [0.98, 0.95, 0.86], [0.93, 0.88, 0.78], [0.82, 0.85, 0.86], [0.55, 0.6, 0.66], [0.25, 0.3, 0.38], [0.36, 0.25, 0.2], [0.2, 0.2, 0.2], PAPER_BLACK];
        ui.horizontal_wrapped(|ui| {
            for p in presets {
                let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                ui.painter().circle_filled(rect.center(), 13.0, srgb(p));
                ui.painter().circle_stroke(rect.center(), 13.0, Stroke::new(1.0, Color32::from_white_alpha(50)));
                if r.clicked() {
                    rgb = p;
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.color_edit_button_rgb(&mut rgb);
            ui.label(RichText::new("Any colour").color(THEME.text_dim));
        });
        ui.add_space(10.0);
        ui.label(RichText::new("Texture").size(12.0).color(THEME.text_dim));
        ui.add(egui::Slider::new(&mut relief, 0.0..=1.0).custom_formatter(|v, _| if v < 0.01 { "plain".into() } else { format!("{:.0}%", v * 100.0) }));
        ui.label(RichText::new("Only the look: the paint feels the grain the same.").size(11.0).color(THEME.text_dim));
    });
    if rgb != before {
        st.engine.set_paper_color(rgb);
    }
    if relief != relief_before {
        st.engine.set_paper_relief(relief);
    }
    if back {
        st.panel = Panel::Layers;
    }
}

fn palette_panel(ctx: &Context, st: &mut Studio) {
    let cur = st.color();
    let mut pick = None;
    side_panel(ctx, "palette", |ui| {
        ui.label(RichText::new(if st.brush_tool == Tool::Ink { "Ink" } else { "Paint" }).size(17.0));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (name, rgb) in pigment::PRESETS {
                let p = pigment::from_srgb(*rgb);
                if swatch(ui, p, 36.0, cur == p).on_hover_text(*name).clicked() {
                    pick = Some(p);
                }
            }
        });
        ui.add_space(10.0);
        // Thinned with water: a weaker wash of the same paint.
        let s = pigment::strength(cur).max(0.02);
        let mut v = s;
        ui.label(RichText::new("Strength").size(12.0).color(THEME.text_dim));
        if ui.add(egui::Slider::new(&mut v, 0.05..=1.0).show_value(false)).changed() {
            pick = Some(cur.map(|x| x / s * v));
        }
        ui.add_space(6.0);
        let (_, mass) = swatch_colors(cur);
        let mut rgb = [mass.r() as f32 / 255.0, mass.g() as f32 / 255.0, mass.b() as f32 / 255.0];
        ui.horizontal(|ui| {
            if ui.color_edit_button_rgb(&mut rgb).changed() {
                pick = Some(pigment::from_srgb(rgb).map(|x| x * s));
            }
            ui.label(RichText::new("Any colour").color(THEME.text_dim));
        });
    });
    if let Some(p) = pick {
        st.set_color(p);
    }
}

fn brush_panel(ctx: &Context, st: &mut Studio) {
    let mut p: BrushP = st.bp();
    let t = st.brush_tool;
    bottom_panel(ctx, "brush", |ui| {
        ui.set_width(300.0);
        let row = |ui: &mut Ui, label: &str, v: &mut f32| {
            ui.horizontal(|ui| {
                ui.add_sized(vec2(100.0, 18.0), egui::Label::new(RichText::new(label).size(13.0).color(THEME.text_dim)));
                ui.add(egui::Slider::new(v, 0.0..=1.0).show_value(false));
            });
        };
        match t {
            Tool::Watercolor => {
                row(ui, "Paint load", &mut p.load);
                row(ui, "Water", &mut p.water);
                row(ui, "Softness", &mut p.softness);
                row(ui, "Bristles", &mut p.bristle);
            }
            Tool::Water => {
                row(ui, "Water", &mut p.water);
                row(ui, "Softness", &mut p.softness);
                row(ui, "Bristles", &mut p.bristle);
            }
            Tool::Ink => {
                row(ui, "Ink load", &mut p.load);
                row(ui, "Bleed", &mut p.bleed);
                row(ui, "Softness", &mut p.softness);
                row(ui, "Splatter", &mut p.splatter);
                row(ui, "Smoothing", &mut p.smoothing);
            }
            Tool::Eraser | Tool::Push => {
                row(ui, "Strength", &mut p.strength);
                row(ui, "Softness", &mut p.softness);
            }
        }
        row(ui, "Pressure: size", &mut p.pressure_size);
        row(ui, "Pressure: flow", &mut p.pressure_flow);
        ui.label(RichText::new("A mouse paints at medium pressure; a tablet pen gives real pressure where the system passes it on.").size(11.0).color(THEME.text_dim));
    });
    if p != st.bp() {
        st.set_bp(p);
    }
}

fn picker_panel(ctx: &Context, st: &mut Studio) {
    let mut wash = st.settings.pick_wash;
    bottom_panel(ctx, "picker", |ui| {
        ui.set_width(300.0);
        ui.radio_value(&mut wash, true, "Wash: whiter means thinner paint of a deeper pigment");
        ui.radio_value(&mut wash, false, "Classic: the colour as it looks");
    });
    if wash != st.settings.pick_wash {
        st.settings.pick_wash = wash;
        st.settings.save();
        st.panel = Panel::None;
    }
}

// ---------------------------------------------------------------- gallery

fn gallery(root: &mut Ui, st: &mut Studio) {
    let ctx = &root.ctx().clone();
    egui::CentralPanel::default().frame(Frame::new().fill(THEME.bg).inner_margin(Margin::symmetric(28, 22))).show(root, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Aqua").size(30.0).color(THEME.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("New painting").color(THEME.bg)).fill(THEME.accent).corner_radius(18)).clicked() {
                    st.dialog = Dialog::New { size: 0, paper: 0, color: 0, plain: false };
                }
                let (_, _, _, tint) = action_look(st, Action::Sync);
                if ic::button(ui, ic::SYNC, 40.0, false, true, tint, "Sync").clicked() {
                    st.dialog = Dialog::Sync { test: String::new() };
                }
            });
        });
        ui.add_space(14.0);
        if st.gallery.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No paintings yet").size(18.0));
                ui.label(RichText::new(format!("Paintings live in {}.\nRun aqua-sync to receive them from your phone.", st.store.root.display())).color(THEME.text_dim));
            });
            return;
        }
        let card_w = 220.0;
        let cols = ((ui.available_width() + 16.0) / (card_w + 16.0)).floor().max(1.0) as usize;
        let mut open = None;
        let mut dup = None;
        let mut del = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("cards").spacing(vec2(16.0, 16.0)).show(ui, |ui| {
                for (i, art) in st.gallery.iter().enumerate() {
                    let tex = thumb(ctx, &mut st.thumbs, &art.dir, art.modified);
                    let aspect = art.height as f32 / art.width.max(1) as f32;
                    let h = (card_w * aspect).clamp(card_w * 0.5, card_w * 1.5);
                    let (rect, r) = ui.allocate_exact_size(vec2(card_w, h), Sense::click());
                    ui.painter().rect_filled(rect, 12.0, THEME.panel_solid);
                    if let Some(t) = tex {
                        let uv = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                        ui.painter().image(t.id(), rect.shrink(1.0), uv, Color32::WHITE);
                    }
                    if r.hovered() {
                        ui.painter().rect_stroke(rect, 12.0, Stroke::new(2.0, THEME.accent), egui::StrokeKind::Outside);
                    }
                    if r.clicked() {
                        open = Some(art.dir.clone());
                    }
                    r.context_menu(|ui| {
                        if ui.button("Duplicate").clicked() {
                            dup = Some(art.dir.clone());
                            ui.close();
                        }
                        if ui.button(RichText::new("Delete").color(THEME.danger)).clicked() {
                            del = Some(art.dir.clone());
                            ui.close();
                        }
                    });
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(d) = dup {
            let _ = st.store.duplicate(&d);
            st.reload_gallery();
        }
        if let Some(d) = del {
            st.dialog = Dialog::Delete(d);
        }
        if let Some(d) = open {
            st.open(d);
        }
    });
}

fn thumb(ctx: &Context, cache: &mut std::collections::HashMap<std::path::PathBuf, (SystemTime, egui::TextureHandle)>, dir: &std::path::Path, modified: SystemTime) -> Option<egui::TextureHandle> {
    if let Some((m, t)) = cache.get(dir)
        && *m == modified
    {
        return Some(t.clone());
    }
    let (w, h, rgba) = crate::store::read_thumb(dir)?;
    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    let t = ctx.load_texture(dir.to_string_lossy(), img, egui::TextureOptions::LINEAR);
    cache.insert(dir.to_path_buf(), (modified, t.clone()));
    Some(t)
}

// ---------------------------------------------------------------- dialogs

fn dialogs(ctx: &Context, st: &mut Studio) {
    match std::mem::replace(&mut st.dialog, Dialog::None) {
        Dialog::None => {}
        Dialog::Sync { test } => st.dialog = sync_dialog(ctx, st, test),
        Dialog::New { size, paper, color, plain } => st.dialog = new_dialog(ctx, st, size, paper, color, plain),
        Dialog::Delete(dir) => {
            let mut keep = true;
            let m = Modal::new(Id::unique("delete")).frame(card_frame().fill(THEME.panel_solid)).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.label(RichText::new("Delete painting?").size(19.0));
                ui.label(RichText::new("This can't be undone.").color(THEME.text_dim));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    if ui.button(RichText::new("Delete").color(THEME.danger)).clicked() {
                        st.store.delete(&dir);
                        st.reload_gallery();
                        keep = false;
                    }
                });
            });
            if keep && !m.should_close() {
                st.dialog = Dialog::Delete(dir);
            }
        }
    }
}

fn status_line(ui: &mut Ui, s: &PushState, idle: &str) {
    let (text, color) = match s {
        PushState::Idle => (idle.to_string(), THEME.text_dim),
        PushState::Sending => ("Sending…".into(), THEME.text_dim),
        PushState::Done(t) => {
            let ago = t.elapsed().unwrap_or_default().as_secs();
            (if ago < 60 { "Synced just now".into() } else { format!("Synced {} min ago", ago / 60) }, THEME.accent)
        }
        PushState::Failed(m) => (m.clone(), THEME.danger),
    };
    ui.label(RichText::new(text).size(12.0).color(color));
}

fn sync_dialog(ctx: &Context, st: &mut Studio, mut test: String) -> Dialog {
    let mut keep = true;
    let status = st.sync.status();
    let before = st.settings.sync.clone();
    let mut sync_now = false;
    let m = Modal::new(Id::unique("sync")).frame(card_frame().fill(THEME.panel_solid)).show(ctx, |ui| {
        ui.set_width(340.0);
        let s = &mut st.settings.sync;
        ui.label(RichText::new("Sync").size(19.0));
        ui.add_space(4.0);
        ui.checkbox(&mut s.on, "Push every save to an aqua-sync daemon on your network");
        ui.add_space(8.0);
        ui.label(RichText::new("Address").size(12.0).color(THEME.text_dim));
        ui.add(egui::TextEdit::singleline(&mut s.primary).hint_text("192.168.1.20").desired_width(f32::INFINITY));
        status_line(ui, &status.primary, "");
        ui.label(RichText::new("Backup address").size(12.0).color(THEME.text_dim));
        ui.add(egui::TextEdit::singleline(&mut s.backup).hint_text("optional").desired_width(f32::INFINITY));
        status_line(ui, &status.backup, "gets each save 5 s after the first");
        ui.label(RichText::new("Key").size(12.0).color(THEME.text_dim));
        ui.add(egui::TextEdit::singleline(&mut s.key).hint_text("if the daemon has one").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Test").clicked() {
                let addrs: Vec<_> = [s.primary.trim(), s.backup.trim()].into_iter().filter(|a| !a.is_empty()).map(String::from).collect();
                test = if addrs.is_empty() {
                    "Type an address first.".into()
                } else {
                    addrs.iter().map(|a| format!("{a}: {}", crate::sync::hello(a).unwrap_or_else(|e| e))).collect::<Vec<_>>().join("\n")
                };
            }
            let can = !s.primary.trim().is_empty() || !s.backup.trim().is_empty();
            if ui.add_enabled(can, egui::Button::new("Sync now")).clicked() {
                sync_now = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    keep = false;
                }
            });
        });
        if !test.is_empty() {
            ui.label(RichText::new(&test).size(12.0).color(THEME.text_dim));
        }
    });
    let s = &mut st.settings.sync;
    s.primary = s.primary.trim().to_string();
    s.backup = s.backup.trim().to_string();
    s.key = s.key.trim().to_string();
    if s.on != before.on || s.primary != before.primary || s.backup != before.backup || s.key != before.key {
        st.settings.save();
    }
    if sync_now {
        if st.dir.is_some() {
            st.save(true);
        } else {
            // From the gallery: every painting.
            for a in st.store.list() {
                st.sync.push(&a.dir, &st.settings.sync, true);
            }
        }
    }
    if keep && !m.should_close() { Dialog::Sync { test } } else { Dialog::None }
}

fn new_dialog(ctx: &Context, st: &mut Studio, mut size: usize, mut paper: u32, mut color: u8, mut plain: bool) -> Dialog {
    let (ww, wh) = st.window;
    let k = (2400.0 / ww.max(wh)).min(1.0);
    let window = ((ww * k) as u32).max(512);
    let window_h = ((wh * k) as u32).max(512);
    let sizes = [("Window", window, window_h), ("Square", 2048, 2048), ("Portrait", 1536, 2048), ("Landscape", 2048, 1536), ("Postcard", 1800, 1200)];
    let mut keep = true;
    let mut create = false;
    let m = Modal::new(Id::unique("new")).frame(card_frame().fill(THEME.panel_solid)).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.label(RichText::new("New painting").size(19.0));
        ui.add_space(6.0);
        ui.label(RichText::new("Size").size(12.0).color(THEME.text_dim));
        for (i, (name, w, h)) in sizes.iter().enumerate() {
            ui.radio_value(&mut size, i, format!("{name}  ·  {w}×{h}"));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Paper").size(12.0).color(THEME.text_dim));
        ui.horizontal(|ui| {
            for (i, name) in ["Cold press", "Hot press", "Rough"].iter().enumerate() {
                ui.selectable_value(&mut paper, i as u32, *name);
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Colour").size(12.0).color(THEME.text_dim));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut color, 0, "Warm white");
            ui.selectable_value(&mut color, 1, "White");
            ui.selectable_value(&mut color, 2, "Black");
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Look").size(12.0).color(THEME.text_dim));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut plain, false, "Textured");
            ui.selectable_value(&mut plain, true, "Plain");
        });
        ui.label(RichText::new("Plain hides the grain; the paint still feels it. Change either any time from the paper under the layers.").size(11.0).color(THEME.text_dim));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                keep = false;
            }
            if ui.add(egui::Button::new(RichText::new("Create").color(THEME.bg)).fill(THEME.accent)).clicked() {
                create = true;
            }
        });
    });
    if create {
        let (_, w, h) = sizes[size];
        st.create(w, h, paper, color, plain);
        return Dialog::None;
    }
    if keep && !m.should_close() { Dialog::New { size, paper, color, plain } } else { Dialog::None }
}
