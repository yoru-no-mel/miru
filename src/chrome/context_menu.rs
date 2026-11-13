//! Right-click context menu on the image: custom-drawn rows with painted
//! icons (same vector-glyph convention as the hotbar), a label, and the
//! numeric shortcut right-aligned. Selected rows get a rounded accent pill.

use egui::{Color32, Pos2, Sense, Shape, Stroke, Vec2};

use crate::filters::FilterMode;
use crate::theme;
use crate::viewer::FitMode;

/// Actions selected from the image right-click context menu.
pub enum ContextMenuAction {
    Filter(FilterMode),
    ActualSize,
    FitWindow,
    FitWidth,
    FitHeight,
    Exit,
}

const ROW_HEIGHT: f32 = 34.0;
const ROW_RADIUS: f32 = 8.0;
const ICON_CX: f32 = 30.0; // icon column center x within a row
const LABEL_X: f32 = 52.0; // label left x within a row
const SHORTCUT_PAD: f32 = 18.0; // gap between shortcut digit and row right edge
const FONT_SIZE: f32 = 15.0;

/// The frame drawn around the menu popup (rounded, bordered, soft shadow).
pub fn menu_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x26, 0x26, 0x26))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(8))
        .shadow(egui::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 2,
            color: Color32::from_black_alpha(110),
        })
}

/// Show the menu contents inside the popup ui.
pub fn show_image_context_menu(
    ui: &mut egui::Ui,
    mode: FitMode,
    filter: FilterMode,
) -> Option<ContextMenuAction> {
    let mut action: Option<ContextMenuAction> = None;
    ui.spacing_mut().item_spacing.y = 2.0;

    // Row width from the widest label so every row (and the popup) shares it.
    let font = egui::FontId::proportional(FONT_SIZE);
    let mut label_w: f32 = 0.0;
    for label in [
        "No Filter",
        "Soften",
        "Soften+Sharpen",
        "Original Size (100%)",
        "Fit in Window",
        "Fit to Height",
        "Fit to Width",
        "Exit",
    ] {
        label_w = label_w.max(ui.painter().layout_no_wrap(label.to_owned(), font.clone(), TEXT).size().x);
    }
    let width = LABEL_X + label_w + 24.0 + font.size * 1.2 + SHORTCUT_PAD;

    let mut item = |ui: &mut egui::Ui,
                    icon: IconKind,
                    label: &str,
                    selected: bool,
                    shortcut: &str,
                    act: ContextMenuAction| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
        if resp.clicked() {
            action = Some(act);
            ui.close();
        }
        let hovered = resp.hovered();
        if selected || hovered {
            let fill = if selected {
                theme::ACCENT_SOFT
            } else {
                mix(MENU_BG, theme::ACCENT_SOFT, 0.45)
            };
            ui.painter().rect_filled(rect, ROW_RADIUS, fill);
        }
        let p = ui.painter();
        let icon_col = if selected || hovered { theme::TEXT } else { TEXT };
        paint_icon(p, icon, Pos2::new(rect.left() + ICON_CX, rect.center().y), icon_col);
        p.text(
            Pos2::new(rect.left() + LABEL_X, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font.clone(),
            TEXT,
        );
        if !shortcut.is_empty() {
            let short_col = if selected {
                mix(theme::TEXT_DIM, theme::TEXT, 0.6)
            } else {
                theme::TEXT_DIM
            };
            p.text(
                Pos2::new(rect.right() - SHORTCUT_PAD, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                shortcut,
                font.clone(),
                short_col,
            );
        }
    };

    item(ui, IconKind::Ban, "No Filter", filter == FilterMode::NoFilter, "1",
        ContextMenuAction::Filter(FilterMode::NoFilter));
    item(ui, IconKind::Drop, "Soften", filter == FilterMode::Soften, "2",
        ContextMenuAction::Filter(FilterMode::Soften));
    item(ui, IconKind::Triangle, "Soften+Sharpen", filter == FilterMode::SoftenSharpen, "3",
        ContextMenuAction::Filter(FilterMode::SoftenSharpen));
    separator(ui, width);
    item(ui, IconKind::Crop, "Original Size (100%)", mode == FitMode::ActualSize, "7",
        ContextMenuAction::ActualSize);
    item(ui, IconKind::FitWindow, "Fit in Window", mode == FitMode::FitWindow, "8",
        ContextMenuAction::FitWindow);
    item(ui, IconKind::FitHeight, "Fit to Height", mode == FitMode::FitHeight, "0",
        ContextMenuAction::FitHeight);
    item(ui, IconKind::FitWidth, "Fit to Width", mode == FitMode::FitWidth, "9",
        ContextMenuAction::FitWidth);
    separator(ui, width);
    item(ui, IconKind::Exit, "Exit", false, "", ContextMenuAction::Exit);
    action
}

const MENU_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
const TEXT: Color32 = theme::TEXT;

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn separator(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 11.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment(
        [Pos2::new(rect.left() + 14.0, y), Pos2::new(rect.right() - 14.0, y)],
        Stroke::new(1.0_f32, theme::SEPARATOR),
    );
}

#[derive(Debug, Clone, Copy)]
enum IconKind {
    Ban,
    Drop,
    Triangle,
    Crop,
    FitWindow,
    FitHeight,
    FitWidth,
    Exit,
}

fn paint_icon(p: &egui::Painter, kind: IconKind, c: Pos2, col: Color32) {
    match kind {
        IconKind::Ban => paint_ban(p, c, col),
        IconKind::Drop => paint_drop(p, c, col),
        IconKind::Triangle => paint_triangle(p, c, col),
        IconKind::Crop => paint_crop(p, c, col),
        IconKind::FitWindow => paint_fit_window(p, c, col),
        IconKind::FitHeight => paint_fit_axis(p, c, col, true),
        IconKind::FitWidth => paint_fit_axis(p, c, col, false),
        IconKind::Exit => paint_exit(p, c, col),
    }
}

fn paint_ban(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    p.circle(c, 7.0, Color32::TRANSPARENT, stroke);
    p.line_segment(
        [c + Vec2::new(-4.9, -4.9), c + Vec2::new(4.9, 4.9)],
        stroke,
    );
}

fn paint_drop(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    let r = 5.2_f32;
    let center = c + Vec2::new(0.0, 2.4);
    let tip = c + Vec2::new(0.0, -6.8);
    let d = center.y - tip.y;
    let beta = (r / d).acos(); // angle at the circle center between up-axis and tangent points
    let tl = center + Vec2::new(-r * beta.sin(), -r * beta.cos());
    let tr = center + Vec2::new(r * beta.sin(), -r * beta.cos());
    // Arc from the left tangent point, through the bottom, to the right one:
    // sweep decreasing from `a0` to `a1` (screen angles, y-down).
    let a0 = (-beta.cos()).atan2(-beta.sin());
    let a1 = (-beta.cos()).atan2(beta.sin()) - std::f32::consts::TAU; // wrap below a0
    let mut pts = Vec::with_capacity(24);
    pts.push(tip);
    pts.push(tl);
    let n = 16;
    for i in 1..n {
        let a = a0 + (a1 - a0) * (i as f32 / n as f32);
        pts.push(center + r * Vec2::new(a.cos(), a.sin()));
    }
    pts.push(tr);
    pts.push(tip);
    p.add(Shape::line(pts, stroke));
}

fn paint_triangle(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    p.add(Shape::line(
        vec![
            c + Vec2::new(0.0, -6.5),
            c + Vec2::new(-6.5, 5.5),
            c + Vec2::new(6.5, 5.5),
            c + Vec2::new(0.0, -6.5),
        ],
        stroke,
    ));
}

fn paint_crop(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    p.add(Shape::line(
        vec![c + Vec2::new(-4.0, -7.5), c + Vec2::new(-4.0, 4.0), c + Vec2::new(7.5, 4.0)],
        stroke,
    ));
    p.add(Shape::line(
        vec![c + Vec2::new(-7.5, -4.0), c + Vec2::new(4.0, -4.0), c + Vec2::new(4.0, 7.5)],
        stroke,
    ));
}

fn paint_fit_window(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    let rect = egui::Rect::from_center_size(c, Vec2::splat(15.0));
    p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
    p.rect_filled(
        egui::Rect::from_min_size(
            c + Vec2::new(-4.4, 0.8),
            Vec2::new(6.8, 4.2),
        ),
        1.2,
        col,
    );
}

fn paint_fit_axis(p: &egui::Painter, c: Pos2, col: Color32, vertical: bool) {
    let stroke = Stroke::new(1.7_f32, col);
    let rect = egui::Rect::from_center_size(c, Vec2::splat(15.0));
    p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
    let (a, b, dx, dy) = if vertical {
        (Vec2::new(0.0, -3.8), Vec2::new(0.0, 3.8), 2.2, 1.8)
    } else {
        (Vec2::new(-3.8, 0.0), Vec2::new(3.8, 0.0), 1.8, 2.2)
    };
    p.line_segment([c + a, c + b], stroke);
    p.line_segment([c + a, c + a + Vec2::new(dx, dy)], stroke);
    p.line_segment([c + a, c + a + Vec2::new(-dx, dy)], stroke);
    p.line_segment([c + b, c + b + Vec2::new(dx, -dy)], stroke);
    p.line_segment([c + b, c + b + Vec2::new(-dx, -dy)], stroke);
}

fn paint_exit(p: &egui::Painter, c: Pos2, col: Color32) {
    let stroke = Stroke::new(1.7_f32, col);
    let r = 7.0_f32;
    let cr = 2.5_f32;
    let gap = 3.2_f32;
    let x0 = c.x - r;
    let x1 = c.x + r;
    let y0 = c.y - r;
    let y1 = c.y + r;
    let arc = |center: Pos2, a0: f32, a1: f32| {
        let n = 6;
        (1..=n)
            .map(|i| {
                let t = a0 + (a1 - a0) * (i as f32 / n as f32);
                center + cr * Vec2::new(t.cos(), t.sin())
            })
            .collect::<Vec<Pos2>>()
    };
    let mut pts = Vec::new();
    // Right edge above the gap, up to the top-right corner…
    pts.push(Pos2::new(x1, c.y - gap));
    pts.push(Pos2::new(x1, y0 + cr));
    // …corner arcs (θ = 0 is the rightmost point, -π/2 the top) and edges.
    pts.extend(arc(Pos2::new(x1 - cr, y0 + cr), 0.0, -std::f32::consts::FRAC_PI_2));
    pts.push(Pos2::new(x0 + cr, y0));
    pts.extend(arc(Pos2::new(x0 + cr, y0 + cr), -std::f32::consts::FRAC_PI_2, -std::f32::consts::PI));
    pts.push(Pos2::new(x0, y1 - cr));
    pts.extend(arc(Pos2::new(x0 + cr, y1 - cr), std::f32::consts::PI, std::f32::consts::FRAC_PI_2));
    pts.push(Pos2::new(x1 - cr, y1));
    pts.extend(arc(Pos2::new(x1 - cr, y1 - cr), std::f32::consts::FRAC_PI_2, 0.0));
    pts.push(Pos2::new(x1, c.y + gap));
    p.add(Shape::line(pts, stroke));

    // Arrow passing through the gap on the right edge.
    let a = c + Vec2::new(-2.5, 0.0);
    let tip = c + Vec2::new(11.0, 0.0);
    p.line_segment([a, tip], stroke);
    p.line_segment([tip, tip + Vec2::new(-3.6, -2.8)], stroke);
    p.line_segment([tip, tip + Vec2::new(-3.6, 2.8)], stroke);
}
