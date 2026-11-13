pub mod context_menu;
pub mod frame;
pub mod hotbar;
pub mod seekbar;
pub mod titlebar;

pub use context_menu::{ContextMenuAction, menu_frame, show_image_context_menu};
pub use hotbar::{HotbarAction, HotbarAction::Seek};
pub use titlebar::TitlebarAction;

use std::time::{Duration, Instant};

/// Visibility + idle-timer engine shared by the top bar and hotbar.
/// Panels track visibility independently.
#[derive(Debug)]
pub struct ChromePanel {
    pub visible: bool,
    pub last_activity: Option<Instant>,
    /// Opacity 0..1 for the 120ms slide/fade animation.
    pub opacity: f32,
    /// How long the panel stays visible after its last poke before hiding.
    /// `Duration::ZERO` = hide the moment the pointer leaves its keep-alive
    /// zone (used by the auto-hide titlebar).
    pub idle_delay: Duration,
}

impl ChromePanel {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            last_activity: None,
            opacity: if visible { 1.0 } else { 0.0 },
            idle_delay: Duration::from_millis(1500),
        }
    }

    pub fn poke(&mut self, now: Instant) {
        self.last_activity = Some(now);
        self.visible = true;
    }
}

#[derive(Debug)]
pub struct ChromeState {
    pub top: ChromePanel,
    pub bottom: ChromePanel,
    pub pinned: bool,
    pub active_interaction: bool,
}

impl ChromeState {
    pub fn new() -> Self {
        let mut top = ChromePanel::new(true);
        // Titlebar + hotbar auto-hide: zero idle delay — each is visible only
        // while the cursor is inside its keep-alive zone (edge strip ∪
        // panel rect).
        top.idle_delay = Duration::ZERO;
        let mut bottom = ChromePanel::new(true);
        bottom.idle_delay = Duration::ZERO;
        Self {
            top,
            bottom,
            pinned: false,
            active_interaction: false,
        }
    }

    pub fn poke_all(&mut self, now: Instant) {
        self.top.poke(now);
        self.bottom.poke(now);
    }

    /// Per-frame update. See [`ChromeFrameInput`]: reveal inputs (pointer in
    /// the 48px edge zones) show a panel; hide inputs (pointer inside the
    /// panel's *actual rect*) keep it alive. The keep-alive zone is the union
    /// of both (each panel has parts its strip doesn't cover: the titlebar is
    /// shorter than the strip, the hotbar pill pokes above it and floats
    /// above a gap below) — with the zero idle delay a one-sided test would
    /// make the panel flicker off while the cursor sits in the uncovered
    /// part.
    /// Pure enough to unit test via [`should_hide`].
    pub fn update(&mut self, now: Instant, input: &ChromeFrameInput) {
        if input.pointer_moved {
            self.poke_all(now);
        } else {
            // Reveal zones still show their own panel.
            if input.reveal_top {
                self.top.poke(now);
            }
            if input.reveal_bottom {
                self.bottom.poke(now);
            }
        }

        let hide_top = should_hide(
            self.top.last_activity,
            now,
            self.top.idle_delay,
            input.inside_top || input.reveal_top,
            self.pinned,
            self.active_interaction,
            input.resizing_or_dragging,
        );
        let hide_bottom = should_hide(
            self.bottom.last_activity,
            now,
            self.bottom.idle_delay,
            input.inside_bottom || input.reveal_bottom,
            self.pinned,
            self.active_interaction,
            input.resizing_or_dragging,
        );
        if hide_top {
            self.top.visible = false;
        }
        if hide_bottom {
            self.bottom.visible = false;
        }

        // 120ms ease animation toward target opacity.
        for panel in [&mut self.top, &mut self.bottom] {
            let target = if panel.visible { 1.0 } else { 0.0 };
            let step = 1.0 / 0.12 * 1.0 / 60.0; // ~per frame at 60fps
            if (panel.opacity - target).abs() <= step {
                panel.opacity = target;
            } else if panel.opacity < target {
                panel.opacity += step;
            } else {
                panel.opacity -= step;
            }
        }
    }
}

impl Default for ChromeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-frame inputs for [`ChromeState::update`].
#[derive(Debug, Default)]
pub struct ChromeFrameInput {
    /// Pointer inside the 48px top / bottom edge reveal zones.
    pub reveal_top: bool,
    pub reveal_bottom: bool,
    /// Pointer inside the panels' actual drawn rects (taller than zones).
    pub inside_top: bool,
    pub inside_bottom: bool,
    /// Any pointer motion this frame (reveals both panels).
    pub pointer_moved: bool,
    /// Window resize / drag-to-move in progress (suspends hiding).
    pub resizing_or_dragging: bool,
}

/// Hide when: idle past delay && pointer outside panel && !pinned &&
/// no active interaction && not resizing/dragging.
pub fn should_hide(
    last_activity: Option<Instant>,
    now: Instant,
    idle_delay: Duration,
    pointer_inside: bool,
    pinned: bool,
    active_interaction: bool,
    resizing_or_dragging: bool,
) -> bool {
    if pinned || active_interaction || resizing_or_dragging || pointer_inside {
        return false;
    }
    match last_activity {
        None => false,
        Some(t) => now.duration_since(t) >= idle_delay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_after_idle() {
        let t0 = Instant::now();
        assert!(should_hide(
            Some(t0),
            t0 + Duration::from_millis(1600),
            Duration::from_millis(1500),
            false,
            false,
            false,
            false
        ));
    }

    #[test]
    fn stays_when_pointer_inside_or_pinned_or_busy() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_secs(10);
        let d = Duration::from_millis(1500);
        assert!(!should_hide(Some(t0), later, d, true, false, false, false));
        assert!(!should_hide(Some(t0), later, d, false, true, false, false));
        assert!(!should_hide(Some(t0), later, d, false, false, true, false));
        assert!(!should_hide(Some(t0), later, d, false, false, false, true));
        assert!(!should_hide(
            Some(t0),
            t0 + Duration::from_millis(100),
            d,
            false,
            false,
            false,
            false
        ));
    }

    #[test]
    fn top_bar_hides_immediately_outside_zone() {
        // Zero idle delay: poked this very frame, but the cursor is outside
        // the keep-alive zone → gone now, no 1.5s grace period.
        let t0 = Instant::now();
        let mut s = ChromeState::new();
        s.top.poke(t0);
        s.update(t0, &ChromeFrameInput::default());
        assert!(!s.top.visible);
    }

    #[test]
    fn top_bar_stays_while_cursor_in_edge_zone() {
        // The 48px reveal strip extends below the 36px bar; the bar must not
        // flicker off while the cursor is between the two.
        let t0 = Instant::now();
        let mut s = ChromeState::new();
        s.update(t0, &ChromeFrameInput { reveal_top: true, ..Default::default() });
        assert!(s.top.visible);
        // Same idleness, pointer truly outside: hides.
        s.update(t0 + Duration::from_secs(10), &ChromeFrameInput::default());
        assert!(!s.top.visible);
    }

    #[test]
    fn panel_rect_keeps_hotbar_alive_past_idle() {
        // Regression: the pill is taller than its 48px reveal zone, so hide
        // decisions must use the panel rect, not the zone.
        let t0 = Instant::now();
        let mut s = ChromeState::new();
        s.poke_all(t0);
        let mut input = ChromeFrameInput {
            inside_bottom: true,
            ..Default::default()
        };
        // Idle, pointer inside the bottom panel rect but outside the reveal
        // zone (reveal flags false, inside reflects the real geometry).
        s.update(t0 + Duration::from_secs(10), &input);
        assert!(s.bottom.visible, "hotbar hid from under the cursor");
        // Same idleness, pointer truly outside: hides.
        input.inside_bottom = false;
        s.update(t0 + Duration::from_secs(20), &input);
        assert!(!s.bottom.visible);
    }
}
