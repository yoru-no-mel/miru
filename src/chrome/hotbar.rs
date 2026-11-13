//! Bottom hotbar pill: one row of painted icon buttons —
//! zoom in/out | rotate ccw/cw | seekbar | counter | prev/next.

use egui::{Pos2, Sense, Shape, Stroke, Vec2};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotbarAction {
    None,
    Menu,
    /// Jump to this playlist index (seekbar scrub/click).
    Seek(u32),
    ZoomIn,
    ZoomOut,
    RotateCcw,
    RotateCw,
    ToggleLock,
    Prev,
    Next,
}

const ICON_SIZE: f32 = 30.0;

pub fn show_hotbar(
    ui: &mut egui::Ui,
    cursor: usize,
    len: usize,
    at_start: bool,
    at_end: bool,
    has_images: bool,
) -> HotbarAction {
    let mut action = HotbarAction::None;
    ui.spacing_mut().item_spacing.x = 6.0;

    let mut seek_target: Option<usize> = None;
    ui.horizontal(|ui| {
        if icon_button(ui, has_images, "Zoom in (+)", paint_zoom_in).clicked() {
            action = HotbarAction::ZoomIn;
        }
        if icon_button(ui, has_images, "Zoom out (−)", paint_zoom_out).clicked() {
            action = HotbarAction::ZoomOut;
        }
        vsep(ui);

        if icon_button(ui, has_images, "Rotate left (Shift+R)", |p, c, col| {
            paint_rotate(p, c, col, true)
        })
        .clicked()
        {
            action = HotbarAction::RotateCcw;
        }
        if icon_button(ui, has_images, "Rotate right (R)", |p, c, col| {
            paint_rotate(p, c, col, false)
        })
        .clicked()
        {
            action = HotbarAction::RotateCw;
        }

        // Seekbar + counter.
        if let Some(idx) = crate::chrome::seekbar::show_seekbar(ui, cursor, len) {
            seek_target = Some(idx);
        }
        let counter = format!("{} / {}", if len > 0 { cursor + 1 } else { 0 }, len);
        ui.label(egui::RichText::new(counter).size(13.0).color(theme::TEXT_DIM));

        vsep(ui);

        if icon_button(
            ui,
            has_images && !at_start,
            "Previous (←)",
            paint_chevron_left,
        )
        .clicked()
        {
            action = HotbarAction::Prev;
        }
        if icon_button(
            ui,
            has_images && !at_end,
            "Next (→)",
            paint_chevron_right,
        )
        .clicked()
        {
            action = HotbarAction::Next;
        }
    });
    if let Some(idx) = seek_target {
        action = HotbarAction::Seek(idx as u32);
    }
    action
}

/// Circular icon button with a painted glyph. Hover shows a subtle disc.
fn icon_button(
    ui: &mut egui::Ui,
    enabled: bool,
    tooltip: &str,
    paint: impl Fn(&egui::Painter, Pos2, egui::Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(ICON_SIZE), Sense::click());
    let resp = resp.on_hover_text(tooltip);
    let color = if enabled {
        if resp.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        }
    } else {
        theme::ICON_DISABLED
    };
    if resp.hovered() && enabled {
        ui.painter()
            .circle_filled(rect.center(), ICON_SIZE / 2.0 - 1.0, theme::CHROME_BG_HOVER);
    }
    if enabled {
        paint(ui.painter(), rect.center(), color);
    }
    resp
}

fn vsep(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_bottom(), rect.center_top()],
        Stroke::new(1.0_f32, theme::SEPARATOR),
    );
    ui.add_space(4.0);
}

fn paint_zoom_in(p: &egui::Painter, c: Pos2, col: egui::Color32) {
    let stroke = Stroke::new(1.5_f32, col);
    p.circle(c, 7.5, egui::Color32::TRANSPARENT, stroke);
    p.line_segment([c - Vec2::new(3.5, 0.0), c + Vec2::new(3.5, 0.0)], stroke);
    p.line_segment([c - Vec2::new(0.0, 3.5), c + Vec2::new(0.0, 3.5)], stroke);
}

fn paint_zoom_out(p: &egui::Painter, c: Pos2, col: egui::Color32) {
    let stroke = Stroke::new(1.5_f32, col);
    p.circle(c, 7.5, egui::Color32::TRANSPARENT, stroke);
    p.line_segment([c - Vec2::new(3.5, 0.0), c + Vec2::new(3.5, 0.0)], stroke);
}

fn paint_rotate(p: &egui::Painter, c: Pos2, col: egui::Color32, ccw: bool) {
    let r = 7.0;
    let stroke = Stroke::new(1.6_f32, col);
    let gap: f32 = std::f32::consts::FRAC_PI_3; // 60° opening at the top
    let start = -gap / 2.0; // screen angle (y down): -30° is upper-right
    let end = start + std::f32::consts::TAU - gap;
    // Mirror about the vertical axis through the center for ccw so the whole
    // icon (arc + arrowhead) is an exact mirror of the cw version.
    let m = |v: Pos2| if ccw { Pos2::new(2.0 * c.x - v.x, v.y) } else { v };
    let mut pts = Vec::with_capacity(24);
    for i in 0..=23 {
        let t = start + (end - start) * (i as f32 / 23.0);
        pts.push(m(c + r * egui::vec2(t.cos(), t.sin())));
    }
    p.add(Shape::line(pts, stroke));

    // Arrowhead at the `end` of the arc, pointing along the direction of
    // travel (increasing screen angle = clockwise; mirrored for ccw).
    let head_at = c + r * egui::vec2(end.cos(), end.sin());
    let tangent = egui::vec2(-end.sin(), end.cos());
    let normal = egui::vec2(tangent.y, -tangent.x);
    let s = 4.0;
    let tip = head_at + tangent * s;
    let left = head_at - tangent * 1.5 + normal * (s * 0.8);
    let right = head_at - tangent * 1.5 - normal * (s * 0.8);
    p.add(Shape::convex_polygon(
        vec![m(tip), m(left), m(right)],
        col,
        Stroke::NONE,
    ));
}

fn paint_chevron_left(p: &egui::Painter, c: Pos2, col: egui::Color32) {
    let stroke = Stroke::new(2.0_f32, col);
    p.add(Shape::line(
        vec![c + Vec2::new(2.5, -5.0), c + Vec2::new(-2.5, 0.0), c + Vec2::new(2.5, 5.0)],
        stroke,
    ));
}

fn paint_chevron_right(p: &egui::Painter, c: Pos2, col: egui::Color32) {
    let stroke = Stroke::new(2.0_f32, col);
    p.add(Shape::line(
        vec![c + Vec2::new(-2.5, -5.0), c + Vec2::new(2.5, 0.0), c + Vec2::new(-2.5, 5.0)],
        stroke,
    ));
}


