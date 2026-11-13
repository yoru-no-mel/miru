//! Generates `miru.icns` for the macOS app bundle: rasterizes the 見
//! brand glyph with the system CJK font at the standard icon sizes and
//! packs the PNGs into an ICNS container (PNG-compressed entries only,
//! valid since macOS 10.7).
//!
//! Usage: `make-icon <output.icns>`

use std::path::PathBuf;

/// (icns entry type, rendered edge length). The `ic1x` / `icxx` families
/// cover modern (PNG-based, 10.7+) entries at every standard size.
const SIZES: &[(&str, u32)] = &[
    ("ic11", 32),    // 16x16 @2x
    ("ic12", 64),    // 32x32 @2x
    ("ic07", 128),
    ("ic13", 256),   // 128x128 @2x
    ("ic08", 256),
    ("ic14", 512),   // 256x256 @2x
    ("ic09", 512),
    ("ic10", 1024),  // 512x512 @2x
];

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "miru.icns".to_owned());
    let out = PathBuf::from(out);

    let mut entries = Vec::new();
    let mut cache: std::collections::HashMap<u32, Vec<u8>> = std::collections::HashMap::new();
    for &(icns_type, size) in SIZES {
        let png = match cache.get(&size) {
            Some(png) => png.clone(),
            None => {
                let Some(icon) = miru::cjk::app_icon_with_background(size as usize, Some([0, 0, 0]))
                else {
                    eprintln!("make-icon: failed to rasterize 見 at {size}px");
                    std::process::exit(1);
                };
                let png = encode_png(icon.width, icon.height, icon.rgba);
                cache.insert(size, png.clone());
                png
            }
        };
        entries.push((icns_type, png));
    }

    let mut icns: Vec<u8> = Vec::new();
    icns.extend_from_slice(b"icns");
    icns.extend_from_slice(&0u32.to_be_bytes()); // patched below
    for (icns_type, png) in &entries {
        icns.extend_from_slice(icns_type.as_bytes());
        icns.extend_from_slice(&((png.len() + 8) as u32).to_be_bytes());
        icns.extend_from_slice(png);
    }
    let total = icns.len() as u32;
    icns[4..8].copy_from_slice(&total.to_be_bytes());

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out, icns).unwrap();
    eprintln!("make-icon: wrote {}", out.display());
}

fn encode_png(w: u32, h: u32, rgba: Vec<u8>) -> Vec<u8> {
    let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(w, h, rgba)
        .expect("icon rgba buffer");
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}
