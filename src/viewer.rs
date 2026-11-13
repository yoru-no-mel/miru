/// Zoom / fit / rotation state for the canvas image.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FitMode {
    FitWindow,
    FitWidth,
    FitHeight,
    ActualSize,
    Free,
}

#[derive(Debug, Clone, Copy)]
pub struct ZoomState {
    pub scale: f32,
    pub offset: egui::Vec2,
    pub fit_mode: FitMode,
    /// Quarter turns clockwise: 0..=3
    pub rotation: u8,
}

impl Default for ZoomState {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: egui::Vec2::ZERO,
            fit_mode: FitMode::FitWindow,
            rotation: 0,
        }
    }
}

impl ZoomState {
    pub fn reset(&mut self) {
        self.scale = 1.0;
        self.offset = egui::Vec2::ZERO;
        self.fit_mode = FitMode::FitWindow;
    }

    pub fn reset_keep_lock(&mut self) {
        // Lock keeps scale/offset/rotation: do nothing.
    }

    pub fn zoom_in(&mut self) {
        self.fit_mode = FitMode::Free;
        self.scale = (self.scale * 1.25).clamp(0.1, 16.0);
    }

    pub fn zoom_out(&mut self) {
        self.fit_mode = FitMode::Free;
        self.scale = (self.scale / 1.25).clamp(0.1, 16.0);
    }

    pub fn zoom_at(&mut self, factor: f32) {
        self.fit_mode = FitMode::Free;
        self.scale = (self.scale * factor).clamp(0.1, 16.0);
    }

    pub fn fit_window(&mut self) {
        self.fit_mode = FitMode::FitWindow;
        self.offset = egui::Vec2::ZERO;
    }

    pub fn fit_width(&mut self) {
        self.fit_mode = FitMode::FitWidth;
        self.offset = egui::Vec2::ZERO;
    }

    pub fn fit_height(&mut self) {
        self.fit_mode = FitMode::FitHeight;
        self.offset = egui::Vec2::ZERO;
    }

    pub fn actual_size(&mut self) {
        self.fit_mode = FitMode::ActualSize;
        self.scale = 1.0;
        self.offset = egui::Vec2::ZERO;
    }

    pub fn rotate_cw(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    pub fn rotate_ccw(&mut self) {
        self.rotation = (self.rotation + 3) % 4;
    }

    pub fn swapped_dimensions(&self) -> bool {
        self.rotation % 2 == 1
    }

    /// Compute the draw size for an image of `img` inside `available`,
    /// honoring fit mode / free zoom. Returns (draw_width, draw_height)
    /// in screen points, accounting for rotation swap.
    pub fn draw_size(&self, img_w: f32, img_h: f32, avail_w: f32, avail_h: f32) -> (f32, f32) {
        if img_w <= 0.0 || img_h <= 0.0 || avail_w <= 0.0 || avail_h <= 0.0 {
            return (0.0, 0.0);
        }
        let (ew, eh) = if self.swapped_dimensions() {
            (img_h, img_w)
        } else {
            (img_w, img_h)
        };
        match self.fit_mode {
            FitMode::FitWindow => {
                let s = (avail_w / ew).min(avail_h / eh);
                (ew * s, eh * s)
            }
            FitMode::FitWidth => {
                let s = avail_w / ew;
                (ew * s, eh * s)
            }
            FitMode::FitHeight => {
                let s = avail_h / eh;
                (ew * s, eh * s)
            }
            FitMode::ActualSize => (ew, eh),
            FitMode::Free => (ew * self.scale, eh * self.scale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_window_scales_to_fit() {
        let z = ZoomState::default();
        // 200x100 image in 100x100 box -> 100x50
        let (w, h) = z.draw_size(200.0, 100.0, 100.0, 100.0);
        assert!((w - 100.0).abs() < 0.01);
        assert!((h - 50.0).abs() < 0.01);
    }

    #[test]
    fn zoom_clamps() {
        let mut z = ZoomState::default();
        for _ in 0..40 {
            z.zoom_in();
        }
        assert!(z.scale <= 16.0);
        for _ in 0..80 {
            z.zoom_out();
        }
        assert!(z.scale >= 0.1);
    }

    #[test]
    fn rotation_swaps_dimensions() {
        let mut z = ZoomState::default();
        assert!(!z.swapped_dimensions());
        z.rotate_cw();
        assert!(z.swapped_dimensions());
        z.rotate_cw();
        assert!(!z.swapped_dimensions());
        z.rotate_ccw();
        assert!(z.swapped_dimensions());
    }

    #[test]
    fn actual_size_ignores_scale() {
        let mut z = ZoomState::default();
        z.actual_size();
        let (w, h) = z.draw_size(300.0, 200.0, 800.0, 600.0);
        assert_eq!((w as i32, h as i32), (300, 200));
    }
}
