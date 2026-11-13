//! CJK glyph support for the 見 brand mark.
//!
//! egui's default fonts contain no CJK glyphs, so a CJK-capable font is
//! loaded from the system font directory at startup and registered as a
//! fallback of the proportional family. The same font bytes rasterize the
//! window (taskbar) icon.

/// System font candidates that contain U+898B (見), in priority order.
#[cfg(windows)]
const FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",    // Microsoft YaHei
    r"C:\Windows\Fonts\msyhbd.ttc",  // Microsoft YaHei Bold
    r"C:\Windows\Fonts\simsun.ttc",  // SimSun
    r"C:\Windows\Fonts\simhei.ttf",  // SimHei
];
#[cfg(target_os = "macos")]
const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",                    // older macOS
    "/System/Library/Fonts/Hiragino Sans GB.ttc",            // newer macOS
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",  // single .ttf fallback
    "/System/Library/Fonts/Supplemental/Songti.ttc",
];
#[cfg(all(unix, not(target_os = "macos")))]
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
];

/// The brand glyph.
pub const MARK: char = '見';

fn font_bytes() -> Option<Vec<u8>> {
    for path in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

/// Register the CJK font as the last fallback of the proportional family,
/// so existing Latin text is untouched and only missing glyphs (like 見)
/// resolve to it.
pub fn install_font(ctx: &egui::Context) {
    let Some(bytes) = font_bytes() else {
        eprintln!("[cjk] no system CJK font found; 見 will render as fallback boxes");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cjk_fallback".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("cjk_fallback".to_owned());
    ctx.set_fonts(fonts);
}

/// Rasterize 見 centered in a `size × size` transparent RGBA icon.
pub fn app_icon(size: usize) -> Option<egui::IconData> {
    app_icon_with_background(size, None)
}

/// Rasterize 見 centered in a `size × size` RGBA icon, optionally composited
/// over a solid background color instead of transparency.
pub fn app_icon_with_background(size: usize, bg: Option<[u8; 3]>) -> Option<egui::IconData> {
    let bytes = font_bytes()?;
    let font = ab_glyph::FontRef::try_from_slice(&bytes).ok()?;
    let id = ab_glyph::Font::glyph_id(&font, MARK);
    let glyph = ab_glyph::Glyph {
        id,
        scale: ab_glyph::PxScale::from(size as f32 * 0.8),
        position: ab_glyph::Point { x: 0.0, y: 0.0 },
    };
    let outlined = ab_glyph::Font::outline_glyph(&font, glyph)?;
    let bounds = outlined.px_bounds();
    let (w, h) = (bounds.width().ceil() as usize, bounds.height().ceil() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    // Coverage buffer for the tight glyph bounds (ab_glyph draws relative
    // to those bounds), then blit centered onto the icon canvas.
    let mut glyph_buf = vec![0u8; w * h * 4];
    outlined.draw(|x, y, v| {
        let i = ((y as usize * w) + x as usize) * 4;
        let a = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        glyph_buf[i..i + 4].copy_from_slice(&[0xE8, 0xE8, 0xE8, a]);
    });
    let mut rgba = match bg {
        Some([r, g, b]) => {
            let mut v = Vec::with_capacity(size * size * 4);
            for _ in 0..size * size {
                v.extend_from_slice(&[r, g, b, 255]);
            }
            v
        }
        None => vec![0u8; size * size * 4],
    };
    let ox = size.saturating_sub(w) / 2;
    let oy = size.saturating_sub(h) / 2;
    for row in 0..h.min(size - oy) {
        for col in 0..w.min(size - ox) {
            let gi = (row * w + col) * 4;
            let gr = glyph_buf[gi];
            let ga = glyph_buf[gi + 3] as u32;
            let di = ((oy + row) * size + ox + col) * 4;
            if bg.is_none() || ga == 255 {
                rgba[di] = gr;
                rgba[di + 3] = ga as u8;
            } else {
                let inv = 255 - ga;
                rgba[di] = ((gr as u32 * ga + rgba[di] as u32 * inv) / 255) as u8;
                rgba[di + 1] = ((gr as u32 * ga + rgba[di + 1] as u32 * inv) / 255) as u8;
                rgba[di + 2] = ((gr as u32 * ga + rgba[di + 2] as u32 * inv) / 255) as u8;
                rgba[di + 3] = 255;
            }
        }
    }
    Some(egui::IconData {
        width: size as u32,
        height: size as u32,
        rgba,
    })
}
