use egui::{Color32, Style, Visuals};

pub const CANVAS_BG: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
pub const CHROME_BG: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
pub const CHROME_BG_HOVER: Color32 = Color32::from_rgb(0x34, 0x34, 0x34);
pub const ACCENT: Color32 = Color32::from_rgb(0xe0, 0x5f, 0xa8);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x8c, 0x3b, 0x68);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);
pub const ICON_DISABLED: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x5a);
pub const BORDER: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
pub const SEPARATOR: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
pub const TRACK_BG: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);
pub const WINDOW_BORDER: Color32 = Color32::from_rgb(0x3c, 0x3c, 0x40);
pub const CLOSE_HOVER: Color32 = Color32::from_rgb(0xe0, 0x47, 0x5a);

#[allow(clippy::field_reassign_with_default)]
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = Style::default();
    style.visuals = Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = CANVAS_BG;
    style.visuals.window_fill = CHROME_BG;
    style.visuals.extreme_bg_color = CANVAS_BG;
    style.visuals.widgets.inactive.bg_fill = CHROME_BG;
    style.visuals.widgets.hovered.bg_fill = CHROME_BG_HOVER;
    style.visuals.widgets.active.bg_fill = CHROME_BG_HOVER;
    style.visuals.selection.bg_fill = ACCENT_SOFT;
    ctx.set_style(style);
}
