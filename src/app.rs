use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::chrome::{ChromeState, ContextMenuAction, HotbarAction, TitlebarAction, menu_frame, show_image_context_menu};
use crate::filters::FilterMode;
use crate::input::{drain_shortcuts, ShortcutAction};
use crate::loader::{Loader, LoaderMsg, TextureCache};
use crate::playlist::Playlist;
use crate::source::{open_source, ImageSource};
use crate::theme;
use crate::viewer::{FitMode, ZoomState};

pub struct App {
    playlist: Playlist,
    source: Option<Arc<dyn ImageSource>>,
    /// The path that was opened (file, folder, or archive): the item used
    /// for `[` / `]` previous/next-source navigation.
    source_path: Option<PathBuf>,
    source_label: String,
    loader: Loader,
    textures: TextureCache,
    /// Full-resolution textures for the zoom tier (extra-large images whose
    /// preview tier was downscaled at decode time).
    full_textures: TextureCache,
    zoom: ZoomState,
    /// Fit mode applied to every newly opened / navigated image; persisted
    /// in the config file so it survives sessions.
    default_fit_mode: FitMode,
    /// Image scaling filter applied to every displayed texture; persisted
    /// in the config file so it survives sessions.
    filter: FilterMode,
    chrome: ChromeState,
    locked: bool,
    maximized: bool,
    fullscreen: bool,
    status: Option<(String, Instant)>,
    last_viewport_size: Option<egui::Vec2>,
    pending_window_size: Option<(f32, f32)>,
    slideshow: bool,
    last_advance: Instant,
    /// Last drawn panel rects (screen space). Used so auto-hide tests the
    /// real panel geometry instead of the smaller reveal zones.
    top_rect: Option<egui::Rect>,
    bottom_rect: Option<egui::Rect>,
    /// Accumulated leftover wheel scroll while sitting at a panning edge
    /// (or on a fit image); crossing +/- WHEEL_NAV_THRESHOLD navigates.
    wheel_accum: f32,
    /// Wheel scroll eases the pan offset toward this target every frame
    /// (exponential smoothing) instead of jumping, so scrolling glides.
    pan_target: egui::Vec2,
    /// Discrete scroll step queued by an Arrow Up/Down press, consumed in
    /// the canvas where the zoom geometry is known. None = no pending key.
    pending_key_scroll: Option<f32>,
}

/// Wheel scroll (points) that must accumulate at an image edge before the
/// viewer moves to the previous/next image. ~2 wheel notches.
const WHEEL_NAV_THRESHOLD: f32 = 100.0;

/// Multiplier applied to raw wheel/scroll deltas for panning speed.
const WHEEL_PAN_SPEED: f32 = 2.5;

/// Points scrolled per Arrow Up/Down key press (raw units, pre-speed).
const KEY_SCROLL_STEP: f32 = 150.0;

/// Exponential smoothing rate (1/s) for wheel panning: higher = snappier.
const PAN_SMOOTH_SPEED: f32 = 18.0;

/// Points of overscroll allowed past an image edge while panning.
const PAN_OVERSCROLL_SLACK: f32 = 40.0;

/// Pan offset bounds relative to the image's rest position. An image that
/// overflows the canvas rests start-aligned (its top / left edge against
/// the canvas edge) instead of centered, so tall images open showing their
/// top; `offset = 0` is that rest position. Negative offsets pan toward
/// the image's end, positive offsets overscroll past its start.
#[derive(Clone, Copy)]
struct PanBounds {
    overflow: egui::Vec2,
    min: egui::Vec2,
    max: egui::Vec2,
}

impl PanBounds {
    fn new(dw: f32, dh: f32, avail: egui::Vec2) -> Self {
        let overflow = egui::vec2((dw - avail.x).max(0.0), (dh - avail.y).max(0.0));
        let slack = egui::Vec2::splat(PAN_OVERSCROLL_SLACK);
        Self {
            overflow,
            min: -overflow - slack,
            max: slack,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        crate::cjk::install_font(&cc.egui_ctx);
        theme::apply_theme(&cc.egui_ctx);
        let persisted = crate::config::load();
        let mut app = Self {
            playlist: Playlist::empty(),
            source: None,
            source_path: None,
            source_label: String::new(),
            loader: Loader::new(),
            textures: TextureCache::new(),
            full_textures: TextureCache::new(),
            zoom: ZoomState::default(),
            default_fit_mode: persisted.fit_mode.unwrap_or(FitMode::FitWindow),
            filter: persisted.filter_mode.unwrap_or_default(),
            chrome: ChromeState::new(),
            locked: false,
            maximized: false,
            fullscreen: false,
            status: None,
            last_viewport_size: None,
            pending_window_size: persisted.window_width.zip(persisted.window_height),
            slideshow: false,
            last_advance: Instant::now(),
            top_rect: None,
            bottom_rect: None,
            wheel_accum: 0.0,
            pan_target: egui::Vec2::ZERO,
            pending_key_scroll: None,
        };
        // Chrome (titlebar + hotbar) starts hidden: each auto-reveals when
        // the cursor enters its edge zone (see chrome auto-hide below).
        app.chrome.top.visible = false;
        app.chrome.bottom.visible = false;
        eprintln!(
            "[startup] persisted_window=({:?}x{:?}) cli_arg={:?}",
            persisted.window_width,
            persisted.window_height,
            initial.as_ref().map(|p| p.display().to_string()),
        );
        // No auto-reopen of a previous session: without a CLI arg the app
        // starts on the empty-state prompt.
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    /// Reset zoom to the persisted default fit mode (used on open/navigate).
    fn reset_zoom(&mut self) {
        self.zoom = ZoomState::default();
        self.zoom.fit_mode = self.default_fit_mode;
        self.pan_target = egui::Vec2::ZERO;
    }

    /// Apply a fit mode now and remember it as the default (persisted).
    fn apply_fit_mode(&mut self, mode: FitMode) {
        match mode {
            FitMode::FitWindow => self.zoom.fit_window(),
            FitMode::FitWidth => self.zoom.fit_width(),
            FitMode::FitHeight => self.zoom.fit_height(),
            FitMode::ActualSize => self.zoom.actual_size(),
            FitMode::Free => {}
        }
        if mode != FitMode::Free {
            self.default_fit_mode = mode;
            self.save_config();
        }
        self.pan_target = self.zoom.offset;
    }

    fn save_config(&self) {
        crate::config::save(&crate::config::PersistedState {
            window_width: self.pending_window_size.map(|(w, _)| w),
            window_height: self.pending_window_size.map(|(_, h)| h),
            fit_mode: Some(self.default_fit_mode),
            filter_mode: Some(self.filter),
        });
    }

    /// Apply a filter mode now and remember it (persisted). Existing
    /// textures were uploaded with the old sampling options, so both
    /// texture caches are dropped and the current decode window is
    /// re-requested; `poll_loader` re-uploads with the new options.
    fn apply_filter_mode(&mut self, mode: FilterMode) {
        if self.filter == mode {
            return;
        }
        self.filter = mode;
        self.textures.clear();
        self.full_textures.clear();
        if !self.playlist.is_empty() {
            self.loader
                .request_around(self.playlist.cursor(), self.playlist.len());
        }
        self.save_config();
        self.set_status(match mode {
            FilterMode::NoFilter => "Filter: No Filter",
            FilterMode::Soften => "Filter: Soften",
            FilterMode::SoftenSharpen => "Filter: Soften+Sharpen",
        });
    }

    fn open_path(&mut self, path: &std::path::Path) {
        eprintln!("[open] opening '{}'", path.display());
        match open_source(path) {
            Ok((source, cursor)) => {
                let entries = source.entries();
                let label = source.source_label();
                let total = entries.len();
                let arc: Arc<dyn ImageSource> = source.into();
                self.source = Some(arc.clone());
                self.source_path = Some(path.to_path_buf());
                self.playlist.replace(entries, cursor);
                self.source_label = label;
                self.textures.clear();
                self.full_textures.clear();
                self.loader.set_source(arc);
                if !self.locked {
                    self.reset_zoom();
                }
                self.loader
                    .request_around(self.playlist.cursor(), self.playlist.len());
                self.save_config();
                self.set_status(format!("Opened {} ({} images)", path.display(), total));
            }
            Err(e) => self.set_status(format!("Cannot open '{}': {e}", path.display())),
        }
    }

    fn navigate_to(&mut self, index: usize) {
        if self.playlist.is_empty() {
            return;
        }
        self.wheel_accum = 0.0;
        self.playlist.seek(index);
        if !self.locked {
            let keep_rotation = false;
            let _ = keep_rotation;
            self.reset_zoom();
        }
        self.loader
            .request_around(self.playlist.cursor(), self.playlist.len());
    }

    fn next(&mut self) {
        self.wheel_accum = 0.0;
        if self.playlist.next() {
            if !self.locked {
                self.reset_zoom();
            }
            self.loader
                .request_around(self.playlist.cursor(), self.playlist.len());
            self.last_advance = Instant::now();
        } else if self.slideshow {
            // Reached the end: stop the slideshow (no wrap in v1).
            self.slideshow = false;
            self.set_status("Slideshow finished (end of list)");
        }
    }

    #[allow(dead_code)] // wired to a hotbar/menu control in a later milestone
    fn toggle_slideshow(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        self.slideshow = !self.slideshow;
        self.last_advance = Instant::now();
        self.set_status(if self.slideshow {
            "Slideshow started (3s)"
        } else {
            "Slideshow stopped"
        });
    }

    fn prev(&mut self) {
        self.wheel_accum = 0.0;
        if self.playlist.prev() {
            if !self.locked {
                self.reset_zoom();
            }
            self.loader
                .request_around(self.playlist.cursor(), self.playlist.len());
        }
    }

    /// `[` / `]`: open the previous / next openable item (image file,
    /// archive, or folder containing images) in the current source's
    /// directory, natural-sorted.
    fn goto_sibling(&mut self, delta: i32) {
        let Some(cur) = self.source_path.clone() else {
            return;
        };
        let (items, idx) = crate::source::fs::openable_siblings(&cur);
        if idx == usize::MAX {
            self.set_status("Current file/folder is gone");
            return;
        }
        let target = if delta < 0 {
            idx.checked_sub(1)
        } else if idx + 1 < items.len() {
            Some(idx + 1)
        } else {
            None
        };
        match target.and_then(|t| items.get(t)) {
            Some(p) => self.open_path(p),
            None => self.set_status(if delta < 0 {
                "No previous file/folder"
            } else {
                "No next file/folder"
            }),
        }
    }

    /// Apply a scroll gesture (already speed-scaled) to the pan target,
    /// with edge navigation: leftover scroll at the top/bottom edge moves
    /// to the previous/next image. `nav_direct`: navigate on the first
    /// scroll when the image fits vertically (used by the arrow keys).
    fn apply_scroll(
        &mut self,
        scroll: egui::Vec2,
        bounds: PanBounds,
        nav_direct: bool,
    ) {
        if bounds.overflow.y > 0.0 {
            let before = self.pan_target.y;
            self.pan_target.y = (self.pan_target.y + scroll.y).clamp(bounds.min.y, bounds.max.y);
            // Leftover scroll, back in raw (unscaled) units, drives the
            // edge-navigation accumulator. target.y = +max.y overscrolls
            // past the top edge, min.y the bottom edge.
            let leftover_raw = (scroll.y - (self.pan_target.y - before)) / WHEEL_PAN_SPEED;
            if leftover_raw > 1.0 && self.pan_target.y >= bounds.max.y - 0.5 {
                // Scrolling up past the top edge: prev image.
                self.wheel_accum += leftover_raw;
                if self.wheel_accum > WHEEL_NAV_THRESHOLD {
                    self.wheel_accum = 0.0;
                    self.prev();
                }
            } else if leftover_raw < -1.0 && self.pan_target.y <= bounds.min.y + 0.5 {
                // Scrolling down past the bottom edge: next image.
                self.wheel_accum += leftover_raw;
                if self.wheel_accum < -WHEEL_NAV_THRESHOLD {
                    self.wheel_accum = 0.0;
                    self.next();
                }
            } else {
                self.wheel_accum = 0.0;
            }
        } else if nav_direct {
            // Image fits vertically: arrows navigate immediately.
            if scroll.y > 0.5 {
                self.prev();
            } else if scroll.y < -0.5 {
                self.next();
            }
        } else {
            // Image fits vertically: accumulate ~2 notches before nav.
            self.wheel_accum += scroll.y / WHEEL_PAN_SPEED;
            if self.wheel_accum > WHEEL_NAV_THRESHOLD {
                self.wheel_accum = 0.0;
                self.prev();
            } else if self.wheel_accum < -WHEEL_NAV_THRESHOLD {
                self.wheel_accum = 0.0;
                self.next();
            }
        }
        // Horizontal overflow pans with scroll.x.
        if bounds.overflow.x > 0.0 {
            self.pan_target.x = (self.pan_target.x + scroll.x).clamp(bounds.min.x, bounds.max.x);
        }
    }

    fn apply_action(&mut self, ctx: &egui::Context, action: ShortcutAction) {
        match action {
            ShortcutAction::Prev => self.prev(),
            ShortcutAction::Next => self.next(),
            ShortcutAction::First => self.navigate_to(0),
            ShortcutAction::Last => {
                let n = self.playlist.len();
                if n > 0 {
                    self.navigate_to(n - 1);
                }
            }
            ShortcutAction::ZoomIn => self.zoom.zoom_in(),
            ShortcutAction::ZoomOut => self.zoom.zoom_out(),
            ShortcutAction::FitWindow => self.apply_fit_mode(FitMode::FitWindow),
            ShortcutAction::FitWidth => self.apply_fit_mode(FitMode::FitWidth),
            ShortcutAction::FitHeight => self.apply_fit_mode(FitMode::FitHeight),
            ShortcutAction::ActualSize => self.apply_fit_mode(FitMode::ActualSize),
            ShortcutAction::RotateCw => self.zoom.rotate_cw(),
            ShortcutAction::RotateCcw => self.zoom.rotate_ccw(),
            ShortcutAction::ToggleLock => {
                self.locked = !self.locked;
                self.chrome.pinned = self.locked;
            }
            ShortcutAction::ToggleFullscreen => self.toggle_fullscreen(ctx),
            ShortcutAction::OpenFile => self.open_file_dialog(),
            ShortcutAction::OpenFolder => self.open_folder_dialog(),
            ShortcutAction::ClosePopoverOrFullscreen => {
                if self.fullscreen {
                    self.set_fullscreen(ctx, false);
                }
            }
            // Queued: applied in the canvas where the zoom geometry lives.
            ShortcutAction::ScrollUp => self.pending_key_scroll = Some(KEY_SCROLL_STEP),
            ShortcutAction::ScrollDown => self.pending_key_scroll = Some(-KEY_SCROLL_STEP),
            ShortcutAction::PrevSource => self.goto_sibling(-1),
            ShortcutAction::NextSource => self.goto_sibling(1),
            ShortcutAction::Filter(mode) => self.apply_filter_mode(mode),
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.set_fullscreen(ctx, !self.fullscreen);
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    fn open_file_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter(
                "Images & archives",
                &[
                    "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp", "zip", "cbz", "7z",
                    "tar.gz", "tgz", "rar", "cbr",
                ],
            )
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"],
            )
            .add_filter(
                "Archives",
                &["zip", "cbz", "7z", "tar.gz", "tgz", "rar", "cbr"],
            )
            .pick_file();
        if let Some(p) = picked {
            self.open_path(&p);
        }
    }

    fn open_folder_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.open_path(&p);
        }
    }

    #[allow(dead_code)] // wired to a menu popover in a later milestone
    fn open_archive_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter(
                "Archives",
                &["zip", "cbz", "7z", "tar.gz", "tgz", "rar", "cbr"],
            )
            .pick_file();
        if let Some(p) = picked {
            self.open_path(&p);
        }
    }

    fn poll_loader(&mut self, ctx: &egui::Context) {
        let tex_options = self.filter.texture_options();
        for msg in self.loader.poll() {
            match msg {
                LoaderMsg::Decoded { index, image, .. } => {
                    let _ = self
                        .textures
                        .insert(ctx, index, &image, tex_options);
                    if index == self.playlist.cursor() {
                        ctx.request_repaint();
                    }
                }
                LoaderMsg::DecodedFull { index, image, .. } => {
                    // Full-res zoom-tier texture: only kept for indices whose
                    // preview was downscaled (all DecodedFull images are).
                    let _ = self
                        .full_textures
                        .insert(ctx, index, &image, tex_options);
                    if index == self.playlist.cursor() {
                        ctx.request_repaint();
                    }
                }
                LoaderMsg::Failed { index, error, .. } => {
                    if index == self.playlist.cursor() {
                        self.set_status(format!(
                            "Cannot decode '{}': {error}",
                            self.entry_name(index)
                        ));
                    }
                }
                LoaderMsg::Skipped { .. } => {}
                // Converted to Decoded/Failed inside `Loader::poll`.
                LoaderMsg::Preloaded { .. } => {}
            }
        }
        // The background preload floods the LRU cache; never let it evict
        // the page being displayed.
        if !self.playlist.is_empty() {
            self.textures.protect(self.playlist.cursor());
            self.full_textures.protect(self.playlist.cursor());
        }
        // Keep requesting while the current texture is missing (e.g. right
        // after open, before the worker answered).
        if !self.playlist.is_empty() && self.textures.get(self.playlist.cursor()).is_none() {
            self.loader
                .request_around(self.playlist.cursor(), self.playlist.len());
            ctx.request_repaint();
        }
    }

    fn entry_name(&self, index: usize) -> String {
        self.playlist
            .entries()
            .get(index)
            .map(|e| e.display_name.clone())
            .unwrap_or_else(|| "?".to_string())
    }

    #[allow(dead_code)] // used by the status/tooltip readout in a later milestone
    fn full_member_name(&self, index: usize) -> String {
        self.playlist
            .entries()
            .get(index)
            .map(|e| match &e.locator {
                crate::source::Locator::File(p) => p.display().to_string(),
                crate::source::Locator::ArchiveMember { member, .. } => member.clone(),
            })
            .unwrap_or_default()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();

        // Losing window focus closes any open popup (the context menu).
        if ctx.input(|i| i.viewport().focused) == Some(false) {
            egui::Popup::close_all(ctx);
        }

        // Drag-drop support: dropping files anywhere on the window opens
        // the first one (files: sibling playlist; folder/archive: playlist).
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(first) = dropped.first() {
            if dropped.len() > 1 {
                self.set_status(format!(
                    "{} files dropped — opened '{}'",
                    dropped.len(),
                    first.display()
                ));
            }
            self.open_path(first);
        }
        let drop_hovered: Vec<String> = ctx.input(|i| {
            i.raw
                .hovered_files
                .iter()
                .map(|f| {
                    f.path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "file".to_string())
                })
                .collect()
        });

        // Keyboard shortcuts.
        for action in drain_shortcuts(ctx) {
            self.apply_action(ctx, action);
        }

        // Mouse wheel zoom at pointer (canvas-level; applied in canvas below
        // via scroll delta — we stash it here for the canvas handler).
        // (Handled inline in the canvas section.)

        // Window border + native resize handles (skipped when maximized
        // or fullscreen). Returns true while a handle is dragged.
        let frame_resizing =
            crate::chrome::frame::show_window_frame(ctx, self.maximized, self.fullscreen);

        // Chrome auto-hide bookkeeping.
        let screen = ctx.screen_rect();
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        let size_changed = match self.last_viewport_size {
            Some(s) => (s - screen.size()).length() > 1.0,
            None => false,
        };
        self.last_viewport_size = Some(screen.size());
        self.pending_window_size = Some((screen.width(), screen.height()));
        // Reveal zones (48px edges) vs. actual panel rects: the hotbar pill
        // is taller than its reveal zone, so hide decisions must use the
        // real rects or the bar vanishes from under the cursor.
        let (reveal_top, reveal_bottom) = match pointer {
            Some(p) => (p.y - screen.top() <= 48.0, screen.bottom() - p.y <= 48.0),
            None => (false, false),
        };
        let inside_top = match (pointer, self.top_rect) {
            (Some(p), Some(r)) => (self.chrome.top.visible || self.chrome.pinned) && r.contains(p),
            _ => false,
        };
        let inside_bottom = match (pointer, self.bottom_rect) {
            (Some(p), Some(r)) => {
                (self.chrome.bottom.visible || self.chrome.pinned) && r.contains(p)
            }
            _ => false,
        };
        // Pressing/dragging on a panel suspends hiding for that panel.
        self.chrome.active_interaction = pointer_down && (inside_top || inside_bottom);
        // NOTE: pointer motion does NOT poke the top bar — the titlebar is
        // revealed only by the cursor entering the top edge zone, and hides
        // (zero idle delay) the moment the cursor leaves it.
        if size_changed || frame_resizing {
            self.chrome.poke_all(now);
        }
        self.chrome.update(
            now,
            &crate::chrome::ChromeFrameInput {
                reveal_top,
                reveal_bottom,
                inside_top,
                inside_bottom,
                pointer_moved: false,
                resizing_or_dragging: size_changed || frame_resizing,
            },
        );
        // Hotbar auto-hides like the titlebar: revealed by the bottom edge
        // zone, hidden the instant the cursor leaves it.
        if self.chrome.top.visible || self.chrome.bottom.visible {
            // Keep the frame alive so the idle timer can hide chrome.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        self.poll_loader(ctx);

        // Decode prefetch continuity: repaint while workers are busy so
        // freshly decoded textures appear promptly.
        // (poll_loader already repaints when current arrives.)

        // Slideshow: auto-advance every 3s, paused while hovering chrome
        // or while zoomed (M5: pause on hover/zoom).
        if self.slideshow {
            let zoomed = !matches!(self.zoom.fit_mode, crate::viewer::FitMode::FitWindow);
            if !zoomed
                && !reveal_top
                && !reveal_bottom
                && now.duration_since(self.last_advance) >= std::time::Duration::from_secs(3)
            {
                self.next();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }

        let has_images = !self.playlist.is_empty();
        let cursor = self.playlist.cursor();
        let len = self.playlist.len();
        let title = if has_images {
            format!(
                "miru — {} — {}/{}",
                self.entry_name(cursor),
                cursor + 1,
                len
            )
        } else {
            "miru".to_string()
        };

        // ---- Titlebar: auto-hide overlay, drawn AFTER the canvas below ----
        // (search for "titlebar overlay"). It is an Area, not a
        // TopBottomPanel, so showing/hiding it never shifts the canvas
        // layout — the image doesn't jump when the bar slides in or out.

        // ---- Canvas (left-drag moves window, middle-drag pans image) ----
        // Titlebar moves via its own panel response above; here the canvas
        // background + image responses forward left-drags to StartDrag.
        // self.bottom_rect still holds last frame's pill rect, used to
        // avoid moving the window when the drag starts on the hotbar pill.
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::CANVAS_BG))
            .show(ctx, |ui| {
                // Full canvas rect, captured BEFORE any allocation: an
                // `allocate_rect` consumes layout space, so centering later
                // against the leftover rect pushed the image to the bottom.
                let full_rect = ui.available_rect_before_wrap();
                let avail = full_rect.size();
                // Background drag surface: covers empty canvas space.
                // `Ui::interact` senses input WITHOUT consuming layout space.
                let bg_resp = ui.interact(
                    full_rect,
                    ui.id().with("canvas_bg_drag"),
                    egui::Sense::drag(),
                );
                let bg_drag_started =
                    bg_resp.drag_started_by(egui::PointerButton::Primary);
                if !has_images {
                    self.pending_key_scroll = None;
                    if bg_drag_started
                        && !self.maximized
                        && !self.fullscreen
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    ui.centered_and_justified(|ui| {
                        ui.style_mut().interaction.selectable_labels = false;
                        ui.label(
                            egui::RichText::new(crate::cjk::MARK)
                                .size(96.0)
                                .color(crate::theme::TEXT_DIM),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Drop an image, folder, or archive  ·  Ctrl+O",
                            )
                            .color(crate::theme::TEXT_DIM),
                        );
                    });
                    return;
                }

                // Current texture (clone handle; cheap Arc).
                let tex = self.textures.get(cursor);
                if let Some((handle, w, h, full_size)) = tex {
                    // Zoom geometry is anchored to the FULL resolution when
                    // the preview was downscaled: the rect never jumps when
                    // the sharp full-res texture arrives; the preview is
                    // simply stretched into the same rect until then.
                    let (zoom_w, zoom_h) = full_size.unwrap_or((w, h));
                    let (dw, dh) =
                        self.zoom
                            .draw_size(zoom_w as f32, zoom_h as f32, avail.x, avail.y);
                    // Zoomed past the preview's native resolution: decode
                    // the full-resolution image in the background. Only
                    // request while it is not cached yet — the cache check
                    // keeps this a one-shot decode instead of re-decoding
                    // and re-uploading the full texture every frame.
                    if full_size.is_some()
                        && dw > w as f32
                        && self.full_textures.get(cursor).is_none()
                    {
                        self.loader
                            .request_full(cursor, self.playlist.len());
                    }
                    // Pan bounds for this frame. The rest position is
                    // start-aligned (top / left edge against the canvas
                    // edge) when the image overflows, so tall images open
                    // showing their top instead of their middle.
                    let bounds = PanBounds::new(dw, dh, avail);
                    // Smooth panning: ease the actual offset toward the
                    // wheel-scroll target every frame (frame-rate
                    // independent exponential smoothing). Repaints keep
                    // coming until the pan settles.
                    {
                        self.pan_target = self
                            .pan_target
                            .clamp(bounds.min, bounds.max);
                        let dist = self.pan_target - self.zoom.offset;
                        if dist.length_sq() > 0.25 {
                            let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05).max(1.0 / 240.0);
                            let k = 1.0 - (-PAN_SMOOTH_SPEED * dt).exp();
                            self.zoom.offset += dist * k;
                            ctx.request_repaint();
                        } else {
                            self.zoom.offset = self.pan_target;
                        }
                    }
                    // Rest position: start-aligned (top / left edge against
                    // the canvas edge) on axes where the image overflows,
                    // centered on axes where it fits.
                    let center = full_rect.center();
                    let rest_min = egui::pos2(
                        if bounds.overflow.x > 0.0 {
                            full_rect.left()
                        } else {
                            center.x - dw / 2.0
                        },
                        if bounds.overflow.y > 0.0 {
                            full_rect.top()
                        } else {
                            center.y - dh / 2.0
                        },
                    );
                    let rect = egui::Rect::from_min_size(
                        rest_min + self.zoom.offset,
                        egui::vec2(dw, dh),
                    );
                    // Image interactions: double-click toggles fit,
                    // left-drag moves the window, middle-drag pans.
                    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                    // Right-click context menu on the image.
                    let mut menu_action: Option<ContextMenuAction> = None;
                    if let Some(resp_menu) = egui::Popup::context_menu(&resp)
                        .frame(menu_frame())
                        .show(|ui| show_image_context_menu(ui, self.zoom.fit_mode, self.filter))
                    {
                        menu_action = resp_menu.inner;
                    }
                    match menu_action {
                        Some(ContextMenuAction::Filter(m)) => self.apply_filter_mode(m),
                        Some(ContextMenuAction::ActualSize) => self.apply_fit_mode(FitMode::ActualSize),
                        Some(ContextMenuAction::FitWindow) => self.apply_fit_mode(FitMode::FitWindow),
                        Some(ContextMenuAction::FitWidth) => self.apply_fit_mode(FitMode::FitWidth),
                        Some(ContextMenuAction::FitHeight) => self.apply_fit_mode(FitMode::FitHeight),
                        Some(ContextMenuAction::Exit) => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        None => {}
                    }
                    if !self.maximized && !self.fullscreen {
                        let left_start = bg_drag_started
                            || resp.drag_started_by(egui::PointerButton::Primary);
                        if left_start {
                            let over_pill = match (
                                ctx.input(|i| i.pointer.latest_pos()),
                                self.bottom_rect,
                            ) {
                                (Some(p), Some(r)) => r.contains(p),
                                _ => false,
                            };
                            if !over_pill {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                        }
                    }
                    if resp.double_clicked() {
                        match self.zoom.fit_mode {
                            crate::viewer::FitMode::FitWindow => self.zoom.actual_size(),
                            _ => self.zoom.fit_window(),
                        }
                        self.pan_target = self.zoom.offset;
                    }
                    // Wheel behavior:
                    // - Ctrl+wheel: zoom anchored at the pointer.
                    // - Plain wheel / Arrow Up+Down: pan the image
                    //   vertically when it is taller than the window;
                    //   continued scroll at the top / bottom edge navigates
                    //   to the previous / next image. If the image fits
                    //   vertically, plain wheel navigates (arrows
                    //   immediately). Horizontal overflow pans too.
                    // Arrow Up/Down queue a discrete step in apply_action;
                    // consume it here where the geometry is known.
                    if let Some(step) = self.pending_key_scroll.take() {
                        self.apply_scroll(egui::vec2(0.0, step), bounds, true);
                    }
                    let scroll = ctx.input(|i| i.smooth_scroll_delta);
                    // Ctrl+wheel / pinch zoom: egui 0.32 routes these to
                    // `zoom_delta()`, NOT `smooth_scroll_delta` (which stays
                    // zero while the zoom modifier is held). Reading the
                    // scroll delta here made Ctrl+wheel zoom fire only off
                    // stale leftover pan deltas, so scaling appeared to lag
                    // behind the scroll.
                    let zoom_factor = ctx.input(|i| i.zoom_delta());
                    if (zoom_factor - 1.0).abs() > 0.001 {
                        let factor = zoom_factor.clamp(0.5, 2.0);
                        // Anchor at pointer: keep the point under the cursor.
                        if let Some(p) = pointer {
                            let before = p - rect.center();
                            self.zoom.zoom_at(factor);
                            let (ndw, _ndh) = self.zoom.draw_size(
                                zoom_w as f32,
                                zoom_h as f32,
                                avail.x,
                                avail.y,
                            );
                            let after_scale = ndw / dw.max(1.0);
                            let after = before * after_scale;
                            self.zoom.offset += before - after;
                        } else {
                            self.zoom.zoom_at(factor);
                        }
                        // Sync the pan target so a pending glide
                        // doesn't drift the image after zooming.
                        self.pan_target = self.zoom.offset;
                    } else if scroll.x != 0.0 || scroll.y != 0.0 {
                        self.apply_scroll(scroll * WHEEL_PAN_SPEED, bounds, false);
                    }
                    // Middle-drag pans the image (left-drag moves the
                    // window, handled above). Pans even at fit.
                    if ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle))
                    {
                        let delta = ctx.input(|i| i.pointer.delta());
                        self.zoom.offset += delta;
                        self.zoom.offset = self.zoom.offset.clamp(bounds.min, bounds.max);
                        // Sync the target: a middle-drag shouldn't be
                        // fought by the wheel-glide animation.
                        self.pan_target = self.zoom.offset;
                    }
                    let angle = self.zoom.rotation as f32 * std::f32::consts::FRAC_PI_2;
                    // Prefer the full-res zoom-tier texture when decoded;
                    // otherwise stretch the preview into the same rect.
                    let draw_handle = match self.full_textures.get(cursor) {
                        Some((full_handle, _, _, _)) => full_handle,
                        None => handle,
                    };
                    let mut image = egui::Image::new(&draw_handle).fit_to_exact_size(rect.size());
                    if angle != 0.0 {
                        image = image.rotate(angle, egui::Vec2::splat(0.5));
                    }
                    ui.put(rect, image);
                } else {
                    // Still decoding: keep the previous frame's space, spinner.
                    self.pending_key_scroll = None;
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                    });
                }
            });

        // ---- Bottom hotbar pill (auto-hide, docked bottom-center) ----
        // Shown only while the cursor is in its keep-alive zone (bottom
        // 48px strip ∪ pill rect) or chrome is pinned. Already an overlay
        // Area, so showing/hiding it never shifts the canvas layout.
        if self.chrome.bottom.visible || self.chrome.pinned {
            let pill = egui::Area::new(egui::Id::new("hotbar"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .movable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(crate::theme::CHROME_BG)
                        .corner_radius(28.0)
                        .stroke(egui::Stroke::new(1.0_f32, crate::theme::BORDER))
                        .shadow(egui::Shadow::NONE)
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            let at_start = self.playlist.at_start();
                            let at_end = self.playlist.at_end();
                            let haction = crate::chrome::hotbar::show_hotbar(
                                ui, cursor, len, at_start, at_end, has_images,
                            );
                            match haction {
                                HotbarAction::Seek(idx) => self.navigate_to(idx as usize),
                                HotbarAction::ZoomIn => self.zoom.zoom_in(),
                                HotbarAction::ZoomOut => self.zoom.zoom_out(),
                                HotbarAction::RotateCcw => self.zoom.rotate_ccw(),
                                HotbarAction::RotateCw => self.zoom.rotate_cw(),
                                HotbarAction::ToggleLock => {
                                    self.locked = !self.locked;
                                    self.chrome.pinned = self.locked;
                                }
                                HotbarAction::Prev => self.prev(),
                                HotbarAction::Next => self.next(),
                                _ => {}
                            }
                        });
                });
            self.bottom_rect = Some(pill.response.rect);
        } else {
            self.bottom_rect = None;
        }

        // ---- Titlebar overlay (auto-hide: drag + min/max/close) ----
        // Shown only while chrome says the top bar is visible (or pinned):
        // the cursor revealed it by entering the top 48px edge zone and it
        // hides the instant the cursor leaves that zone (zero idle delay).
        // Drawn after the CentralPanel so its widgets are registered last in
        // the Middle layer — egui's hit-test picks the last widget under the
        // pointer, so the bar wins over the canvas drag surface beneath it.
        // An Area (not a TopBottomPanel): the canvas layout is unaffected,
        // so the image never jumps when the bar appears or disappears.
        if self.chrome.top.visible || self.chrome.pinned {
            let bar_rect = egui::Rect::from_min_size(
                screen.min,
                egui::vec2(screen.width(), crate::chrome::titlebar::TITLEBAR_HEIGHT),
            );
            let title_resp = egui::Area::new(egui::Id::new("titlebar"))
                .movable(false)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    // Clamp the Area's ui to the bar strip: show_titlebar's
                    // drag surface spans `available_rect_before_wrap`, which
                    // inside a raw Area would otherwise cover the whole
                    // window and eat every canvas interaction.
                    ui.set_width(bar_rect.width());
                    ui.set_height(crate::chrome::titlebar::TITLEBAR_HEIGHT);
                    let tout =
                        crate::chrome::titlebar::show_titlebar(ui, &title, self.maximized);
                    match tout.action {
                        TitlebarAction::Minimize => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        TitlebarAction::ToggleMaximize => {
                            self.maximized = !self.maximized;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
                        }
                        TitlebarAction::Close => {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        TitlebarAction::None => {}
                    }
                    if tout.drag_started && !self.maximized && !self.fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if tout.double_clicked {
                        self.maximized = !self.maximized;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
                    }
                });
            // top_rect is just the titlebar now (used for chrome bookkeeping
            // next frame: keep-alive while the cursor hovers the bar).
            self.top_rect = Some(title_resp.response.rect);
        } else {
            self.top_rect = None;
        }

        // ---- Status toast: brief message floating above the pill ----
        if let Some((msg, t)) = &self.status {
            if now.duration_since(*t) < std::time::Duration::from_secs(5) {
                egui::Area::new(egui::Id::new("status-toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -86.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(crate::theme::CHROME_BG)
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .size(12.0)
                                        .color(crate::theme::TEXT_DIM),
                                );
                            });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // ---- Drop overlay: shown while files hover the window ----
        if !drop_hovered.is_empty() {
            egui::Area::new(egui::Id::new("drop-overlay"))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let r = ctx.screen_rect();
                    let _ = ui.allocate_rect(r, egui::Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(r, 0.0, egui::Color32::from_black_alpha(140));
                    painter.rect_stroke(
                        r.shrink(24.0),
                        12.0,
                        egui::Stroke::new(2.0_f32, crate::theme::ACCENT),
                        egui::StrokeKind::Middle,
                    );
                    painter.text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        format!(
                            "Drop to open\n{}",
                            drop_hovered.first().cloned().unwrap_or_default()
                        ),
                        egui::FontId::proportional(20.0),
                        crate::theme::TEXT,
                    );
                });
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let (w, h) = self.pending_window_size.unwrap_or((1280.0, 800.0));
        self.pending_window_size = Some((w, h));
        self.save_config();
    }
}
