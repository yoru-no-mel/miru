//! Seekbar: painter-drawn track with accent fill and a white thumb.
//! Returns the newly requested index when the user clicks/scrubs.

use egui::{CursorIcon, Rect, Sense, Vec2};

use crate::theme;

const HEIGHT: f32 = 22.0; // grab target height
const TRACK_H: f32 = 4.0;
const THUMB_R: f32 = 7.0;
const DESIRED_WIDTH: f32 = 380.0;

pub fn show_seekbar(ui: &mut egui::Ui, cursor: usize, len: usize) -> Option<usize> {
    if len <= 1 {
        // Single image: render a static empty track, not interactive.
        let width = DESIRED_WIDTH.min(ui.available_width() * 0.6);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::hover());
        let track = Rect::from_center_size(rect.center(), Vec2::new(width, TRACK_H));
        ui.painter().rect_filled(track, TRACK_H / 2.0, theme::TRACK_BG);
        return None;
    }
    let max = (len - 1) as f32;
    let screen_w = ui.ctx().screen_rect().width();
    let width = DESIRED_WIDTH.max(screen_w.min(1200.0) - 620.0).min(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::click_and_drag());
    resp.clone().on_hover_cursor(CursorIcon::PointingHand);

    let track = Rect::from_center_size(rect.center(), Vec2::new(width, TRACK_H));
    let frac_at = |x: f32| ((x - track.min.x) / track.width()).clamp(0.0, 1.0);

    let mut frac = cursor as f32 / max;
    let dragging = resp.dragged() || resp.drag_started();
    if dragging {
        if let Some(p) = resp.interact_pointer_pos() {
            frac = frac_at(p.x);
        }
    }

    let painter = ui.painter();
    painter.rect_filled(track, TRACK_H / 2.0, theme::TRACK_BG);
    let fill_x = track.min.x + frac * track.width();
    let fill = Rect::from_min_max(track.min, egui::Pos2::new(fill_x, track.max.y));
    if fill.width() > 0.5 {
        painter.rect_filled(fill, TRACK_H / 2.0, theme::ACCENT);
    }
    let thumb_x = (track.min.x + THUMB_R).max(fill_x).min(track.max.x - THUMB_R);
    let thumb_color = if resp.hovered() || dragging {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    painter.circle_filled(egui::Pos2::new(thumb_x, track.center().y), THUMB_R, thumb_color);

    // Manually allocated responses never report `changed()`, so detect
    // press/drag directly: click-to-jump + drag scrubbing.
    let active = resp.drag_started() || resp.dragged() || resp.clicked();
    if active {
        if let Some(p) = resp.interact_pointer_pos() {
            let f = frac_at(p.x);
            return Some(((f * max).round() as usize).min(len - 1));
        }
    }
    None
}
