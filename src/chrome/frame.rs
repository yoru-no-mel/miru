use std::cell::RefCell;

use egui::{CursorIcon, LayerId, Order, Pos2, Rect, Sense, Vec2, ViewportCommand};

use crate::theme;

/// Width of the invisible resize grab area along each window edge.
const GRAB: f32 = 8.0;

/// Minimum window size (must mirror the `min_inner_size` set in main.rs).
const MIN_SIZE: Vec2 = Vec2::new(480.0, 320.0);

/// Whether the native `BeginResize` command is supported by the OS backend.
/// winit implements `drag_resize_window` on Windows, X11 and Wayland; the
/// macOS appkit backend returns `NotSupported`, so it falls back to
/// manual resize.
const NATIVE_RESIZE: bool = !cfg!(target_os = "macos");

/// In-progress manual resize: the zone that started the drag, the pointer
/// position at start, and the window's outer rect at start (egui points).
struct ManualResize {
    zone: usize,
    dir: egui::ResizeDirection,
    start_pointer: Pos2,
    start_outer: Rect,
    start_inner: Rect,
}

thread_local! {
    static MANUAL_RESIZE: RefCell<Option<ManualResize>> = const { RefCell::new(None) };
}

/// Anchor-side flags: which edges of the window the drag pulls.
impl ManualResize {
    fn pulls_east(&self) -> bool {
        matches!(self.dir, egui::ResizeDirection::East | egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthEast)
    }

    fn pulls_west(&self) -> bool {
        matches!(self.dir, egui::ResizeDirection::West | egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthWest)
    }

    fn pulls_south(&self) -> bool {
        matches!(self.dir, egui::ResizeDirection::South | egui::ResizeDirection::SouthEast | egui::ResizeDirection::SouthWest)
    }

    fn pulls_north(&self) -> bool {
        matches!(self.dir, egui::ResizeDirection::North | egui::ResizeDirection::NorthEast | egui::ResizeDirection::NorthWest)
    }

    /// One frame of manual resize: derive the new outer rect from the
    /// pointer delta, then express it as InnerSize + OuterPosition
    /// commands (egui converts both from points to physical pixels).
    fn apply(&self, ctx: &egui::Context, pointer: Pos2) {
        let d = pointer - self.start_pointer;
        let border = self.start_outer.size() - self.start_inner.size();

        let new_outer_w = if self.pulls_east() {
            (self.start_outer.width() + d.x).max(MIN_SIZE.x)
        } else if self.pulls_west() {
            (self.start_outer.width() - d.x).max(MIN_SIZE.x)
        } else {
            self.start_outer.width()
        };
        let new_outer_h = if self.pulls_south() {
            (self.start_outer.height() + d.y).max(MIN_SIZE.y)
        } else if self.pulls_north() {
            (self.start_outer.height() - d.y).max(MIN_SIZE.y)
        } else {
            self.start_outer.height()
        };

        let pos = Pos2::new(
            if self.pulls_west() {
                self.start_outer.max.x - new_outer_w
            } else {
                self.start_outer.min.x
            },
            if self.pulls_north() {
                self.start_outer.max.y - new_outer_h
            } else {
                self.start_outer.min.y
            },
        );

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
            (new_outer_w - border.x).max(1.0),
            (new_outer_h - border.y).max(1.0),
        )));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
    }
}

/// Paints the 1px window border and installs native resize handles on every
/// edge and corner.
///
/// Undecorated windows get no OS resize border, so without this the window
/// cannot be scaled. Dragging any zone issues
/// `ViewportCommand::BeginResize`, letting the OS run the resize natively
/// (no manual size/position math, no DPI pitfalls). macOS does not support
/// native programmatic resize (`drag_resize_window` is unimplemented in
/// winit's appkit backend), so there the drag drives a manual
/// size/position update anchored to the opposite corner instead.
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

                if NATIVE_RESIZE {
                    if resp.drag_started() {
                        // OS takes over the resize from here; egui frames
                        // resume once the user releases the mouse button.
                        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
                    }
                    resizing = resizing || resp.dragged();
                } else {
                    resizing = resizing || manual_resize(ctx, i, &resp, dir);
                }
            });
    }
    resizing
}

/// Manual resize fallback for platforms whose winit backend does not
/// implement `drag_resize_window` (macOS). State lives across frames so the
/// start geometry can be anchored; only the zone that owns the drag may
/// touch or end it (every other zone's response reports not-dragged).
fn manual_resize(
    ctx: &egui::Context,
    zone: usize,
    resp: &egui::Response,
    dir: egui::ResizeDirection,
) -> bool {
    let mut active = MANUAL_RESIZE.with(|s| s.borrow().is_some());
    if resp.drag_started() {
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        if let (Some(pointer), Some(outer), Some(inner)) = (pointer, outer, inner) {
            MANUAL_RESIZE.with(|s| {
                *s.borrow_mut() = Some(ManualResize {
                    zone,
                    dir,
                    start_pointer: pointer,
                    start_outer: outer,
                    start_inner: inner,
                });
            });
            active = true;
        }
    }
    let owns = MANUAL_RESIZE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|m| m.zone == zone)
            .unwrap_or(false)
    });
    if owns {
        if resp.dragged() {
            let pointer = ctx.input(|i| i.pointer.interact_pos());
            if let Some(pointer) = pointer {
                MANUAL_RESIZE.with(|s| s.borrow().as_ref().unwrap().apply(ctx, pointer));
            }
            active = true;
        } else if !resp.drag_started() {
            // Pointer released: end the drag.
            MANUAL_RESIZE.with(|s| *s.borrow_mut() = None);
        }
    }
    active
}
