pub mod archive;
pub mod fs;
pub(crate) mod rar;
pub(crate) mod sevenz;
pub(crate) mod targz;
pub(crate) mod zip;

pub use archive::{open_source, ArchiveSource};
pub use fs::FsSource;

use std::path::{Path, PathBuf};

/// A decoded image ready for texture upload.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Original dimensions when this decode was downscaled (preview tier).
    /// `None` when the decode is at full resolution.
    pub full_size: Option<(u32, u32)>,
}

impl DecodedImage {
    pub fn byte_len(&self) -> usize {
        self.rgba.len()
    }
}

#[derive(Debug, Clone)]
pub enum SourceError {
    Io(String),
    Unsupported(String),
    Archive(String),
    Decode(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) => write!(f, "I/O error: {s}"),
            Self::Unsupported(s) => write!(f, "Unsupported: {s}"),
            Self::Archive(s) => write!(f, "Archive error: {s}"),
            Self::Decode(s) => write!(f, "Decode error: {s}"),
        }
    }
}

impl std::error::Error for SourceError {}

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    TarGz,
    Rar,
}

impl ArchiveFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::TarGz => "tar.gz",
            Self::Rar => "rar",
        }
    }
}

/// Map a path to an archive format (case-insensitive).
/// `.cbz` -> Zip, `.cbr` -> Rar.
pub fn archive_format_for_path(path: &Path) -> Option<ArchiveFormat> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".zip") || name.ends_with(".cbz") {
        return Some(ArchiveFormat::Zip);
    }
    if name.ends_with(".7z") {
        return Some(ArchiveFormat::SevenZ);
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(ArchiveFormat::TarGz);
    }
    if name.ends_with(".rar") || name.ends_with(".cbr") {
        return Some(ArchiveFormat::Rar);
    }
    None
}

pub fn is_archive_path(path: &Path) -> bool {
    archive_format_for_path(path).is_some()
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            IMAGE_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

pub fn is_image_member_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            IMAGE_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub enum Locator {
    File(PathBuf),
    ArchiveMember { archive: PathBuf, member: String },
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub display_name: String,
    pub locator: Locator,
}

impl Entry {
    pub fn file(path: PathBuf) -> Self {
        let display_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        Self {
            display_name,
            locator: Locator::File(path),
        }
    }

    pub fn archive_member(archive: PathBuf, member: String) -> Self {
        // Display the file name portion, tooltip can show full member path.
        let display_name = Path::new(&member)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&member)
            .to_string();
        Self {
            display_name,
            locator: Locator::ArchiveMember { archive, member },
        }
    }
}

/// Natural-sort entries by display name (case-insensitive), falling back to
/// full member path order for stability.
pub fn sort_entries_naturally(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        natord::compare_ignore_case(&a.display_name, &b.display_name)
            .then_with(|| natord::compare(&a.display_name, &b.display_name))
    });
}

/// Image source: list once at open time, decode on demand.
/// Implementations must be `Send + Sync` so decode workers can share them.
pub trait ImageSource: Send + Sync {
    fn entries(&self) -> Vec<Entry>;
    fn load(&self, index: usize) -> Result<DecodedImage, SourceError>;
    fn source_label(&self) -> String;

    /// Background full preload: decode every entry once, emitting
    /// `(index, result)` in order. The emitter returns `false` to abort
    /// (source replaced / app closed). Default: sequential `load()` calls;
    /// formats where random access is expensive (solid/streaming archives)
    /// should override with a cheaper single streaming pass.
    fn preload(&self, emit: &mut dyn FnMut(usize, Result<DecodedImage, SourceError>) -> bool) {
        for index in 0..self.entries().len() {
            if !emit(index, self.load(index)) {
                break;
            }
        }
    }

    /// Full-resolution decode for on-demand sharp zooming. The default
    /// preview tier downscales very large images (see `decode_bytes`); this
    /// tier is only requested when the user zooms past the preview's native
    /// resolution. Default: same as `load` (preview is already full-res).
    fn load_full(&self, index: usize) -> Result<DecodedImage, SourceError> {
        self.load(index)
    }
}

/// Preview tier cap: longest edge decoded for the always-shown texture.
/// Keeps memory + texture upload fast for huge images.
pub const MAX_PREVIEW_EDGE: u32 = 3840;
/// Full-tier hard cap (GPU texture limits): modern GPUs support 16384px
/// textures; anything larger still gets clamped to this.
pub const MAX_FULL_EDGE: u32 = 16384;

/// Decode raw image bytes to RGBA8 (preview tier: downscales very large
/// images, reporting the original size in `DecodedImage::full_size`).
pub fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, SourceError> {
    let dyn_img =
        image::load_from_memory(bytes).map_err(|e| SourceError::Decode(format!("{e}")))?;
    let (w, h) = (dyn_img.width(), dyn_img.height());
    let img = if w.max(h) > MAX_PREVIEW_EDGE {
        let scale = MAX_PREVIEW_EDGE as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale) as u32).max(1);
        let nh = ((h as f32 * scale) as u32).max(1);
        dyn_img.thumbnail(nw, nh)
    } else {
        dyn_img
    };
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Ok(DecodedImage {
        rgba: rgba.into_raw(),
        width,
        height,
        full_size: (width != w || height != h).then_some((w, h)),
    })
}

/// Full-resolution decode (zoom tier). Only clamped at the GPU texture
/// limit; `full_size` stays `None` since the result is already full-res.
pub fn decode_bytes_full(bytes: &[u8]) -> Result<DecodedImage, SourceError> {
    let dyn_img =
        image::load_from_memory(bytes).map_err(|e| SourceError::Decode(format!("{e}")))?;
    let (w, h) = (dyn_img.width(), dyn_img.height());
    let img = if w.max(h) > MAX_FULL_EDGE {
        let scale = MAX_FULL_EDGE as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale) as u32).max(1);
        let nh = ((h as f32 * scale) as u32).max(1);
        dyn_img.thumbnail(nw, nh)
    } else {
        dyn_img
    };
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Ok(DecodedImage {
        rgba: rgba.into_raw(),
        width,
        height,
        full_size: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_format_mapping() {
        assert_eq!(
            archive_format_for_path(Path::new("a.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.CBZ")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.7z")),
            Some(ArchiveFormat::SevenZ)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.TGZ")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.rar")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            archive_format_for_path(Path::new("a.cbr")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(archive_format_for_path(Path::new("a.png")), None);
        assert_eq!(archive_format_for_path(Path::new("a")), None);
    }

    #[test]
    fn natural_sort_orders_numbers_numerically() {
        // Plan requirement: `2_0001` before `10_0002`.
        let mut entries = vec![
            Entry::file(PathBuf::from("10_0002.webp")),
            Entry::file(PathBuf::from("2_0001.webp")),
        ];
        sort_entries_naturally(&mut entries);
        let names: Vec<_> = entries.iter().map(|e| e.display_name.clone()).collect();
        assert_eq!(names, vec!["2_0001.webp", "10_0002.webp"]);

        // Corpus-realistic zero-padded prefixes sort numerically too.
        let mut entries = vec![
            Entry::file(PathBuf::from("10_0009.webp")),
            Entry::file(PathBuf::from("02_0001.webp")),
            Entry::file(PathBuf::from("06_0005.webp")),
        ];
        sort_entries_naturally(&mut entries);
        let names: Vec<_> = entries.iter().map(|e| e.display_name.clone()).collect();
        assert_eq!(names, vec!["02_0001.webp", "06_0005.webp", "10_0009.webp"]);
    }

    #[test]
    fn image_extension_filter() {
        assert!(is_image_path(Path::new("photo.JPG")));
        assert!(is_image_path(Path::new("a.webp")));
        assert!(!is_image_path(Path::new("a.txt")));
        assert!(!is_image_path(Path::new("a")));
        assert!(is_image_member_name("folder/01.PNG"));
        assert!(!is_image_member_name("folder/readme.md"));
    }

    /// Preview tier downscales oversized images and reports the original
    /// size; full tier keeps the native resolution.
    #[test]
    fn preview_downscales_but_full_keeps_resolution() {
        // 4000x10 solid-color PNG: cheap to encode/decode.
        let buf = image::RgbaImage::from_pixel(4000, 10, image::Rgba([10, 20, 30, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(buf)
            .write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageFormat::Png,
            )
            .unwrap();

        let preview = decode_bytes(&png).unwrap();
        // `thumbnail` fits inside the (3840, 9) box, preserving ratio.
        assert!(preview.width <= MAX_PREVIEW_EDGE);
        assert_ne!((preview.width, preview.height), (4000, 10));
        assert_eq!(preview.full_size, Some((4000, 10)));

        let full = decode_bytes_full(&png).unwrap();
        assert_eq!((full.width, full.height), (4000, 10));
        assert_eq!(full.full_size, None);

        // Images under the cap are never downscaled.
        let small = image::RgbaImage::from_pixel(100, 50, image::Rgba([1, 2, 3, 255]));
        let mut png2 = Vec::new();
        image::DynamicImage::ImageRgba8(small)
            .write_to(
                &mut std::io::Cursor::new(&mut png2),
                image::ImageFormat::Png,
            )
            .unwrap();
        let preview2 = decode_bytes(&png2).unwrap();
        assert_eq!((preview2.width, preview2.height), (100, 50));
        assert_eq!(preview2.full_size, None);
    }
}
