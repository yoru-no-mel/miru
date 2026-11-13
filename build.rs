use std::env;
use std::path::Path;

const MARK: char = '見';

const FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\msyh.ttc",
    r"C:\Windows\Fonts\msyhbd.ttc",
    r"C:\Windows\Fonts\simsun.ttc",
    r"C:\Windows\Fonts\simhei.ttf",
];

const GLYPH_COLOR: [u8; 3] = [0xE8, 0xE8, 0xE8];
const ICO_SIZES: &[usize] = &[16, 32, 48, 64, 128, 256];

fn font_bytes() -> Option<Vec<u8>> {
    for path in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}

fn render_rgba(size: usize, font_bytes: &[u8]) -> Option<Vec<u8>> {
    let font = ab_glyph::FontRef::try_from_slice(font_bytes).ok()?;
    let id = ab_glyph::Font::glyph_id(&font, MARK);
    let glyph = ab_glyph::Glyph {
        id,
        scale: ab_glyph::PxScale::from(size as f32 * 0.8),
        position: ab_glyph::Point { x: 0.0, y: 0.0 },
    };
    let outlined = ab_glyph::Font::outline_glyph(&font, glyph)?;
    let bounds = outlined.px_bounds();
    let (w, h) = (
        bounds.width().ceil() as usize,
        bounds.height().ceil() as usize,
    );
    if w == 0 || h == 0 {
        return None;
    }
    let mut glyph_buf = vec![0u8; w * h * 4];
    outlined.draw(|x, y, v| {
        let i = ((y as usize * w) + x as usize) * 4;
        let a = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        glyph_buf[i..i + 4].copy_from_slice(&[GLYPH_COLOR[0], GLYPH_COLOR[1], GLYPH_COLOR[2], a]);
    });
    let mut rgba = vec![0u8; size * size * 4];
    let ox = size.saturating_sub(w) / 2;
    let oy = size.saturating_sub(h) / 2;
    for row in 0..h.min(size - oy) {
        let src = (row * w) * 4;
        let dst = ((oy + row) * size + ox) * 4;
        let span = (w.min(size - ox)) * 4;
        rgba[dst..dst + span].copy_from_slice(&glyph_buf[src..src + span]);
    }
    Some(rgba)
}

fn ico_entry_bmp(rgba: &[u8], size: usize) -> Vec<u8> {
    let xor_len = size * size * 4;
    let mask_row = ((size + 31) / 32) * 4;
    let mask_len = mask_row * size;

    let mut out = Vec::with_capacity(40 + xor_len + mask_len);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(xor_len as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);

    for y in (0..size).rev() {
        for x in 0..size {
            let i = (y * size + x) * 4;
            out.push(rgba[i + 2]);
            out.push(rgba[i + 1]);
            out.push(rgba[i]);
            out.push(rgba[i + 3]);
        }
    }
    out.extend(std::iter::repeat(0u8).take(mask_len));
    out
}

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Some(font) = font_bytes() else {
        println!("cargo:warning=No system CJK font found; exe icon will not be embedded");
        return;
    };

    let mut images = Vec::new();
    for &size in ICO_SIZES {
        match render_rgba(size, &font) {
            Some(rgba) => images.push(ico_entry_bmp(&rgba, size)),
            None => {
                println!("cargo:warning=Could not rasterize {MARK} at {size}px");
                return;
            }
        }
    }

    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + images.len() * 16;
    for (i, img) in images.iter().enumerate() {
        let size = ICO_SIZES[i];
        let w = if size >= 256 { 0u8 } else { size as u8 };
        ico.push(w);
        ico.push(w);
        ico.push(0);
        ico.push(0);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += img.len();
    }
    for img in &images {
        ico.extend_from_slice(img);
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let ico_path = Path::new(&out_dir).join("miru.ico");
    std::fs::write(&ico_path, &ico).unwrap();

    winresource::WindowsResource::new()
        .set_icon(ico_path.to_str().unwrap())
        .compile()
        .unwrap();
}
