//! The web app's line icons (web/www/js/icons.js), drawn with egui's painter:
//! SVG path data on a 24x24 grid, stroked like the Android outlined icons.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2, vec2};

pub const BACK: &[&str] = &["M19 12H5", "M11 6l-6 6 6 6"];
pub const UNDO: &[&str] = &["M9 14L4 9l5-5", "M4 9h11a5 5 0 0 1 0 10h-4"];
pub const REDO: &[&str] = &["M15 14l5-5-5-5", "M20 9H9a5 5 0 0 0 0 10h4"];
pub const DRY: &[&str] = &["M3 8h10a3 3 0 1 0-3-3", "M3 12h15a3 3 0 1 1-3 3", "M3 16h7"];
pub const SHARE: &[&str] = &["M12 3v12", "M7 8l5-5 5 5", "M5 13v6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6"];
pub const LAYERS: &[&str] = &["M12 3l9 5-9 5-9-5z", "M3 12l9 5 9-5", "M3 16l9 5 9-5"];
pub const BRUSH: &[&str] = &[
    "M19.5 3.5c1 1 1 2 0 3L12 14l-2-2 7.5-7.5c1-1 2-1 3 0z",
    "M10 12c-3 0-5 2-5 4.5 0 1.3-.8 2.3-2 2.8 3.5 1 8 0 8.5-3.8.2-1.4-.3-2.6-1.5-3.5z",
];
pub const DROP: &[&str] = &["M12 3s-7 7.5-7 12a7 7 0 0 0 14 0c0-4.5-7-12-7-12z"];
pub const QUILL: &[&str] = &["M20 3C13 4 8 8 6.5 15L5 21", "M20 3c-1 6-5 11-12.5 12", "M10 10.5l3 3"];
pub const ERASER: &[&str] = &["M14.5 4.5L20 10l-9 9H7l-3.5-3.5z", "M9 10l5.5 5.5", "M11 19h9.5"];
pub const PUSH: &[&str] = &["M3 15c2-2 4-2 6 0s4 2 6 0", "M3 19.5c2-2 4-2 6 0s4 2 6 0", "M11 8h9", "M17 5l3 3-3 3"];
pub const WASH_PICK: &[&str] = &["M16.5 3.5l4 4", "M18.5 5.5L9 15l-2.5.5.5-2.5 9.5-9.5", "M5 17s-2 2.2-2 3.3A2 2 0 0 0 5 22a2 2 0 0 0 2-1.7C7 19.2 5 17 5 17z"];
pub const DROPPER: &[&str] = &["M14 6l4 4", "M15.5 4.5a2.1 2.1 0 0 1 3 0l1 1a2.1 2.1 0 0 1 0 3L17 11l-4-4z", "M13 8l-8 8v3h3l8-8"];
pub const ENDLESS: &[&str] = &["M12 12c-2-2.5-4-4-6-4a4 4 0 0 0 0 8c2 0 4-1.5 6-4s4-4 6-4a4 4 0 0 1 0 8c-2 0-4-1.5-6-4z"];
pub const CUP: &[&str] = &["M4 7h12v8a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z", "M16 9.5h1.5a2.5 2.5 0 0 1 0 5H16", "M6.5 11c1-.8 2-.8 3 0s2 .8 3 0"];
pub const ADD: &[&str] = &["M12 5v14", "M5 12h14"];
pub const TRASH: &[&str] = &["M5 7h14", "M10 7V4h4v3", "M7 7l1 13h8l1-13", "M10 11v6", "M14 11v6"];
pub const EYE: &[&str] = &["M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z", "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z"];
pub const EYE_OFF: &[&str] = &["M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z", "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z", "M3 3l18 18"];
pub const UP: &[&str] = &["M12 19V5", "M6 11l6-6 6 6"];
pub const DOWN: &[&str] = &["M12 5v14", "M6 13l6 6 6-6"];
pub const CLEAR_LAYER: &[&str] = &["M12 3l9 5-9 5-9-5z", "M3 12l9 5 9-5", "M15 15l6 6", "M21 15l-6 6"];
pub const MORE: &[&str] = &["M12 5.5v.01", "M12 12v.01", "M12 18.5v.01"];
pub const SYNC: &[&str] = &["M20 11a8 8 0 0 0-14.5-4.5L4 8", "M4 4v4h4", "M4 13a8 8 0 0 0 14.5 4.5L20 16", "M20 20v-4h-4"];

/// Draws `paths` filling `rect`.
pub fn paint(ui: &Ui, rect: Rect, paths: &[&str], color: Color32) {
    let k = rect.width() / 24.0;
    let stroke = Stroke::new((1.8 * k).max(1.2), color);
    let p = ui.painter_at(rect.expand(2.0));
    for d in paths {
        for line in parse(d) {
            let pts: Vec<Pos2> = line.iter().map(|v| rect.min + *v * k).collect();
            if pts.len() == 2 && pts[0].distance(pts[1]) < 0.5 {
                // A dot ("v.01"), drawn round.
                p.circle_filled(pts[0], stroke.width * 0.75, color);
            } else {
                p.line(pts, stroke);
            }
        }
    }
}

/// Polylines for one SVG path (M L H V C S Q A Z, absolute and relative).
fn parse(d: &str) -> Vec<Vec<Vec2>> {
    let toks = tokens(d);
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    let mut cur = Vec::new();
    let (mut p, mut start, mut last_ctrl) = (Vec2::ZERO, Vec2::ZERO, None::<Vec2>);
    let mut i = 0;
    let mut cmd = 'M';
    let num = |i: &mut usize| -> f32 {
        let v = match toks.get(*i) {
            Some(Tok::Num(n)) => *n,
            _ => 0.0,
        };
        *i += 1;
        v
    };
    while i < toks.len() {
        if let Tok::Cmd(c) = toks[i] {
            cmd = c;
            i += 1;
            if c == 'z' || c == 'Z' {
                cur.push(start);
                p = start;
                last_ctrl = None;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { p } else { Vec2::ZERO };
        match cmd.to_ascii_uppercase() {
            'M' => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                p = base + vec2(num(&mut i), num(&mut i));
                start = p;
                cur.push(p);
                // Further pairs are line-tos.
                cmd = if rel { 'l' } else { 'L' };
                last_ctrl = None;
            }
            'L' => {
                p = base + vec2(num(&mut i), num(&mut i));
                cur.push(p);
                last_ctrl = None;
            }
            'H' => {
                let x = num(&mut i);
                p = vec2(if rel { p.x + x } else { x }, p.y);
                cur.push(p);
                last_ctrl = None;
            }
            'V' => {
                let y = num(&mut i);
                p = vec2(p.x, if rel { p.y + y } else { y });
                cur.push(p);
                last_ctrl = None;
            }
            'C' | 'S' => {
                let c1 = if cmd.eq_ignore_ascii_case(&'C') {
                    base + vec2(num(&mut i), num(&mut i))
                } else {
                    last_ctrl.map_or(p, |c| p * 2.0 - c)
                };
                let c2 = base + vec2(num(&mut i), num(&mut i));
                let e = base + vec2(num(&mut i), num(&mut i));
                for k in 1..=12 {
                    let t = k as f32 / 12.0;
                    let u = 1.0 - t;
                    cur.push(p * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + e * (t * t * t));
                }
                last_ctrl = Some(c2);
                p = e;
            }
            'Q' => {
                let c = base + vec2(num(&mut i), num(&mut i));
                let e = base + vec2(num(&mut i), num(&mut i));
                for k in 1..=10 {
                    let t = k as f32 / 10.0;
                    let u = 1.0 - t;
                    cur.push(p * (u * u) + c * (2.0 * u * t) + e * (t * t));
                }
                p = e;
                last_ctrl = None;
            }
            'A' => {
                let (rx, ry, rot) = (num(&mut i), num(&mut i), num(&mut i));
                let (large, sweep) = (num(&mut i) != 0.0, num(&mut i) != 0.0);
                let e = base + vec2(num(&mut i), num(&mut i));
                arc(&mut cur, p, e, rx, ry, rot, large, sweep);
                p = e;
                last_ctrl = None;
            }
            _ => i += 1,
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// SVG's endpoint arc, as points (the spec's F.6.5 conversion).
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Vec2>, p0: Vec2, p1: Vec2, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-4 || ry < 1e-4 {
        out.push(p1);
        return;
    }
    let (s, c) = rot_deg.to_radians().sin_cos();
    let d = (p0 - p1) / 2.0;
    let x1 = c * d.x + s * d.y;
    let y1 = -s * d.x + c * d.y;
    let l = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den.max(1e-9)).sqrt();
    if large == sweep {
        k = -k;
    }
    let cx1 = k * rx * y1 / ry;
    let cy1 = -k * ry * x1 / rx;
    let mid = (p0 + p1) / 2.0;
    let center = vec2(c * cx1 - s * cy1 + mid.x, s * cx1 + c * cy1 + mid.y);
    let ang = |ux: f32, uy: f32| uy.atan2(ux);
    let a0 = ang((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut da = ang((-x1 - cx1) / rx, (-y1 - cy1) / ry) - a0;
    let tau = std::f32::consts::TAU;
    if sweep && da < 0.0 {
        da += tau;
    } else if !sweep && da > 0.0 {
        da -= tau;
    }
    let n = ((da.abs() / tau) * 32.0).ceil().max(2.0) as usize;
    for k in 1..=n {
        let a = a0 + da * k as f32 / n as f32;
        let (sa, ca) = a.sin_cos();
        let (x, y) = (rx * ca, ry * sa);
        out.push(center + vec2(c * x - s * y, s * x + c * y));
    }
}

enum Tok {
    Cmd(char),
    Num(f32),
}

fn tokens(d: &str) -> Vec<Tok> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let ch = b[i] as char;
        if ch.is_ascii_alphabetic() {
            out.push(Tok::Cmd(ch));
            i += 1;
        } else if ch == '-' || ch == '.' || ch.is_ascii_digit() {
            let st = i;
            i += 1;
            let mut dot = ch == '.';
            while i < b.len() {
                let c = b[i] as char;
                if c.is_ascii_digit() {
                    i += 1;
                } else if c == '.' && !dot {
                    dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Tok::Num(d[st..i].parse().unwrap_or(0.0)));
        } else {
            i += 1;
        }
    }
    out
}

/// A round icon button. `tint` overrides the usual colours.
pub fn button(ui: &mut Ui, paths: &[&str], size: f32, selected: bool, enabled: bool, tint: Option<Color32>, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let theme = crate::ui::Theme::get();
    if selected {
        ui.painter().circle_filled(rect.center(), size / 2.0, theme.accent_soft);
    } else if enabled && resp.hovered() {
        ui.painter().circle_filled(rect.center(), size / 2.0, Color32::from_white_alpha(14));
    }
    let color = if !enabled {
        theme.text_dim.gamma_multiply(0.35)
    } else if let Some(t) = tint {
        t
    } else if selected {
        theme.accent
    } else {
        theme.text
    };
    let s = size * 0.5;
    paint(ui, Rect::from_center_size(rect.center(), vec2(s, s)), paths, color);
    resp.on_hover_text(tip)
}

pub fn center_rect(c: Pos2, s: f32) -> Rect {
    Rect::from_center_size(c, vec2(s, s))
}
