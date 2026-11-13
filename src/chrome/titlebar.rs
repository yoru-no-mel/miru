//! Custom undecorated titlebar: app icon (left), centered title,
//! min/max/close (right). All chrome is painted with the egui painter
//! (no font glyphs), so every icon renders identically on any platform.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme;

/// Window-control actions from the custom titlebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    None,
    Minimize,
    ToggleMaximize,
    Close,
}

/// Everything [`show_titlebar`] needs its caller to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitlebarOutput {
    /// Which window-control button was clicked (if any).
    pub action: TitlebarAction,
    /// Left-press started a drag on empty titlebar space (buttons excluded —
    /// they win the press since they are allocated after the drag surface).
    pub drag_started: bool,
    /// Double-click on the drag surface (toggle maximize).
    pub double_clicked: bool,
}

/// Height of the titlebar row.
pub const TITLEBAR_HEIGHT: f32 = 36.0;

/// Width of each window-control hit target.
const BTN_W: f32 = 44.0;

pub fn show_titlebar(ui: &mut egui::Ui, title: &str, maximized: bool) -> TitlebarOutput {
    let mut action = TitlebarAction::None;

    // Drag surface under everything else in the row.
    let bar_rect = ui.available_rect_before_wrap();
    // Bar background: as an overlay Area the titlebar has no panel frame,
    // so it fills its own strip (occluding a tall image underneath).
    ui.painter().rect_filled(bar_rect, 0.0, theme::CANVAS_BG);
    let drag_surface = ui.interact(
        bar_rect,
        ui.id().with("titlebar_drag"),
        Sense::click_and_drag(),
    );

    // Title painted centered on the window, directly with the painter (not a
    // layout widget) so it can never push the icon / window buttons around —
    // layout is identical on every OS regardless of title length.
    ui.painter().text(
        bar_rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0),
        theme::TEXT_DIM,
    );

    // horizontal_centered: allocates the full panel height so the window
    // buttons center vertically.
    ui.horizontal_centered(|ui| {
        // Right-aligned window buttons, painted glyphs.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if window_button(ui, "close", BTN_W, |p, c, col| paint_close(p, c, col)).clicked() {
                action = TitlebarAction::Close;
            }
            if window_button(ui, "maximize", BTN_W, |p, c, col| {
                paint_maximize(p, c, col, maximized)
            })
            .clicked()
            {
                action = TitlebarAction::ToggleMaximize;
            }
            if window_button(ui, "minimize", BTN_W, paint_minimize).clicked() {
                action = TitlebarAction::Minimize;
            }
        });
    });

    TitlebarOutput {
        action,
        drag_started: drag_surface.drag_started_by(egui::PointerButton::Primary),
        double_clicked: drag_surface.double_clicked(),
    }
}

/// A full-height window-control hit target with a painted glyph.
/// Hover tint follows egui's style; close gets the red tint via its paint fn.
fn window_button(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    paint: impl Fn(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(width, TITLEBAR_HEIGHT), Sense::click());
    let center = rect.center();
    let painter = ui.painter();
    if resp.hovered() {
        let hover = if id == "close" {
            theme::CLOSE_HOVER
        } else {
            theme::CHROME_BG_HOVER
        };
        painter.rect_filled(rect, 0.0, hover);
    }
    let color = if resp.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    paint(painter, center, color);
    resp.on_hover_text(match id {
        "close" => "Close",
        "maximize" => "Maximize / Restore",
        _ => "Minimize",
    })
}

fn paint_minimize(p: &egui::Painter, c: Pos2, col: Color32) {
    p.line_segment([c - Vec2::new(5.0, 0.0), c + Vec2::new(5.0, 0.0)], Stroke::new(1.2_f32, col));
}

fn paint_maximize(p: &egui::Painter, c: Pos2, col: Color32, maximized: bool) {
    let stroke = Stroke::new(1.2_f32, col);
    if maximized {
        // Restore: two offset frames.
        let front = Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::splat(9.0));
        p.rect(front, 2.0, Color32::TRANSPARENT, stroke, egui::StrokeKind::Middle);
        let back = Rect::from_center_size(c - Vec2::new(1.5, 1.5), Vec2::splat(9.0));
        p.line_segment(
            [back.left_top() + Vec2::new(2.0, 0.0), back.right_top() + Vec2::new(0.0, 0.0)],
            stroke,
        );
        p.line_segment(
            [back.left_top(), back.left_bottom() - Vec2::new(0.0, 2.0)],
            stroke,
        );
    } else {
        let r = Rect::from_center_size(c, Vec2::splat(10.0));
        p.rect(r, 2.0, Color32::TRANSPARENT, stroke, egui::StrokeKind::Middle);
    }
}

fn paint_close(p: &egui::Painter, c: Pos2, col: Color32) {
    let d = 4.5;
    let stroke = Stroke::new(1.2_f32, col);
    p.line_segment([c - Vec2::splat(d), c + Vec2::splat(d)], stroke);
    p.line_segment(
        [c + Vec2::new(-d, d), c + Vec2::new(d, -d)],
        stroke,
    );
}

