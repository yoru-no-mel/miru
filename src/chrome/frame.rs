use egui::{CursorIcon, LayerId, Order, Pos2, Rect, Sense, Vec2, ViewportCommand};

use crate::theme;

/// Width of the invisible resize grab area along each window edge.
const GRAB: f32 = 8.0;

/// Paints the 1px window border and installs native resize handles on every
/// edge and corner.
///
/// Undecorated windows get no OS resize border, so without this the window
/// cannot be scaled. Dragging any zone issues
/// `ViewportCommand::BeginResize`, letting the OS run the resize natively
/// (no manual size/position math, no DPI pitfalls).
///
/// Layout note: each zone gets its OWN tiny `Area`. A single screen-spanning
/// Area puts a full-screen "move" widget in a top layer, and egui's hit-test
/// then filters out every panel/widget behind it — titlebar buttons, hotbar
/// and canvas clicks all die. Small areas keep the interactive widgets as
/// small as the grab strips themselves.
///
/// Returns true while a resize handle is being dragged, so the caller can
/// keep the chrome visible.
pub fn show_window_frame(ctx: &egui::Context, maximized: bool, fullscreen: bool) -> bool {
    if maximized || fullscreen {
        return false;
    }
    let screen = ctx.screen_rect();

    // Visible 1px border so the window edge reads against any background.
    let painter = ctx.layer_painter(LayerId::new(
        Order::Foreground,
        "window-border".into(),
    ));
    painter.rect_stroke(
        screen.shrink(0.5),
        0.0,
        egui::Stroke::new(1.0_f32, theme::WINDOW_BORDER),
        egui::StrokeKind::Middle,
    );

    let zones: [(Rect, CursorIcon, egui::ResizeDirection); 8] = [
        (
            Rect::from_min_size(screen.min, Vec2::splat(GRAB)),
            CursorIcon::ResizeNwSe,
            egui::ResizeDirection::NorthWest,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.max.x - GRAB, screen.min.y),
                Vec2::splat(GRAB),
            ),
            CursorIcon::ResizeNeSw,
            egui::ResizeDirection::NorthEast,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.min.x, screen.max.y - GRAB),
                Vec2::splat(GRAB),
            ),
            CursorIcon::ResizeNeSw,
            egui::ResizeDirection::SouthWest,
        ),
        (
            Rect::from_min_size(screen.max - Vec2::splat(GRAB), Vec2::splat(GRAB)),
            CursorIcon::ResizeNwSe,
            egui::ResizeDirection::SouthEast,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.min.x + GRAB, screen.min.y),
                Vec2::new((screen.width() - 2.0 * GRAB).max(0.0), GRAB),
            ),
            CursorIcon::ResizeVertical,
            egui::ResizeDirection::North,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.min.x + GRAB, screen.max.y - GRAB),
                Vec2::new((screen.width() - 2.0 * GRAB).max(0.0), GRAB),
            ),
            CursorIcon::ResizeVertical,
            egui::ResizeDirection::South,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.min.x, screen.min.y + GRAB),
                Vec2::new(GRAB, (screen.height() - 2.0 * GRAB).max(0.0)),
            ),
            CursorIcon::ResizeHorizontal,
            egui::ResizeDirection::West,
        ),
        (
            Rect::from_min_size(
                Pos2::new(screen.max.x - GRAB, screen.min.y + GRAB),
                Vec2::new(GRAB, (screen.height() - 2.0 * GRAB).max(0.0)),
            ),
            CursorIcon::ResizeHorizontal,
            egui::ResizeDirection::East,
        ),
    ];

    let mut resizing = false;
    for (i, (rect, cursor, dir)) in zones.into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("window-frame-zone", i)))
            .order(Order::Foreground)
            .movable(false)
            // `interactable(false)` keeps the Area's implicit move-widget
            // hover-only, so clicks on the strip pass through (there is
            // nothing behind anyway) instead of being eaten.
            .interactable(false)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let resp = ui
                    .allocate_exact_size(rect.size(), Sense::drag())
                    .1
                    .on_hover_cursor(cursor);
                if resp.drag_started() {
                    // OS takes over the resize from here; egui frames resume
                    // once the user releases the mouse button.
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
                }
                resizing = resizing || resp.dragged();
            });
    }
    resizing
}
