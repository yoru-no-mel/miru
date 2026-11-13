use std::path::{Path, PathBuf};

use super::{
    decode_bytes, decode_bytes_full, is_image_path, sort_entries_naturally, DecodedImage, Entry,
    ImageSource, SourceError,
};

/// Filesystem source: a folder, or a single file (sibling playlist).
pub struct FsSource {
    entries: Vec<Entry>,
    label: String,
}

impl FsSource {
    /// Open a folder (non-recursive, v1).
    pub fn open_folder(path: &Path) -> Result<(Self, usize), SourceError> {
        let entries = list_image_files(path)?;
        if entries.is_empty() {
            return Err(SourceError::Unsupported(format!(
                "No images found in folder '{}'",
                path.display()
            )));
        }
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("folder")
            .to_string();
        Ok((Self { entries, label }, 0))
    }

    /// Open a single file: playlist = natural-sorted image siblings,
    /// cursor starts at the opened file (Honeyview behavior).
    pub fn open_file(path: &Path) -> Result<(Self, usize), SourceError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), |p| p.to_path_buf());
        let entries = list_image_files(&parent)?;
        if entries.is_empty() {
            return Err(SourceError::Unsupported(format!(
                "No images found in '{}'",
                parent.display()
            )));
        }
        let wanted = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let cursor = entries
            .iter()
            .position(|e| e.display_name.eq_ignore_ascii_case(wanted))
            .unwrap_or(0);
        let label = parent
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("folder")
            .to_string();
        Ok((Self { entries, label }, cursor))
    }
}

fn list_image_files(dir: &Path) -> Result<Vec<Entry>, SourceError> {
    let rd = std::fs::read_dir(dir)
        .map_err(|e| SourceError::Io(format!("Cannot list '{}': {e}", dir.display())))?;
    let mut entries = Vec::new();
    for item in rd {
        let item = item.map_err(|e| SourceError::Io(format!("{e}")))?;
        let p = item.path();
        if p.is_file() && is_image_path(&p) {
            entries.push(Entry::file(p));
        }
    }
    sort_entries_naturally(&mut entries);
    Ok(entries)
}

/// True when the folder directly contains at least one image file
/// (non-recursive, matching `open_folder`).
pub fn folder_has_images(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|item| is_image_path(&item.path())))
        .unwrap_or(false)
}

/// Openable items for the `[` / `]` source navigation: image files,
/// archives, and folders containing images, in the directory of `cur`,
/// natural-sorted by name. Returns `(items, index_of_cur)`; the index is
/// `usize::MAX` when `cur` is no longer listed (deleted / renamed).
pub fn openable_siblings(cur: &Path) -> (Vec<PathBuf>, usize) {
    let parent = cur
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let cur_key = cur.to_string_lossy().to_lowercase();
    let mut items: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&parent) {
        for item in rd.flatten() {
            let p = item.path();
            let openable = if p.is_dir() {
                folder_has_images(&p)
            } else {
                is_image_path(&p) || super::is_archive_path(&p)
            };
            if openable {
                items.push(p);
            }
        }
    }
    items.sort_by(|a, b| {
        let an = a
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bn = b
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        natord::compare_ignore_case(&an, &bn).then_with(|| natord::compare(&an, &bn))
    });
    let idx = items
        .iter()
        .position(|p| p.to_string_lossy().to_lowercase() == cur_key)
        .unwrap_or(usize::MAX);
    (items, idx)
}

impl ImageSource for FsSource {
    fn entries(&self) -> Vec<Entry> {
        self.entries.clone()
    }

    fn load(&self, index: usize) -> Result<DecodedImage, SourceError> {
        let bytes = self.read_entry_bytes(index)?;
        decode_bytes(&bytes)
    }

    fn load_full(&self, index: usize) -> Result<DecodedImage, SourceError> {
        let bytes = self.read_entry_bytes(index)?;
        decode_bytes_full(&bytes)
    }

    fn source_label(&self) -> String {
        self.label.clone()
    }
}

impl FsSource {
    fn read_entry_bytes(&self, index: usize) -> Result<Vec<u8>, SourceError> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| SourceError::Unsupported(format!("Index {index} out of range")))?;
        let path = match &entry.locator {
            super::Locator::File(p) => p.clone(),
            super::Locator::ArchiveMember { .. } => {
                return Err(SourceError::Unsupported("Not a file entry".into()));
            }
        };
        std::fs::read(&path)
            .map_err(|e| SourceError::Io(format!("Cannot read '{}': {e}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_sandbox(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("vi_fs_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn folder_has_images_checks_direct_files() {
        let base = setup_sandbox("has_images");
        let empty = base.join("empty");
        let full = base.join("full");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("x.png"), b"x").unwrap();
        assert!(!folder_has_images(&empty));
        assert!(folder_has_images(&full));
        // Non-recursive: nested images don't count.
        let nested = base.join("nested");
        std::fs::create_dir_all(nested.join("inner")).unwrap();
        std::fs::write(nested.join("inner").join("y.jpg"), b"x").unwrap();
        assert!(!folder_has_images(&nested));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn openable_siblings_lists_sorted_with_index() {
        let base = setup_sandbox("siblings");
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("1.png"), b"x").unwrap();
        std::fs::write(base.join("b.png"), b"x").unwrap();
        std::fs::write(base.join("a.zip"), b"x").unwrap();
        std::fs::write(base.join("c.txt"), b"x").unwrap();
        std::fs::create_dir_all(base.join("noimg")).unwrap();
        let name = |p: &Path| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        };
        // Opened the archive: index 0 of [a.zip, b.png, sub].
        let (items, idx) = openable_siblings(&base.join("a.zip"));
        let names: Vec<String> = items.iter().map(|p| name(p)).collect();
        assert_eq!(names, vec!["a.zip", "b.png", "sub"]);
        assert_eq!(idx, 0);
        // Opened the image: index 1.
        let (_, idx) = openable_siblings(&base.join("b.png"));
        assert_eq!(idx, 1);
        // Opened the folder: index 2.
        let (_, idx) = openable_siblings(&base.join("sub"));
        assert_eq!(idx, 2);
        // Missing current item -> sentinel index.
        let (_, idx) = openable_siblings(&base.join("gone.zip"));
        assert_eq!(idx, usize::MAX);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Unpadded numbers must sort 1, 2, 3, 11 — never 1, 11, 2, 3 —
    /// both in the playlist and in the `[` / `]` source listing.
    #[test]
    fn unpadded_numbers_sort_naturally() {
        let base = setup_sandbox("natural");
        for n in ["1", "2", "3", "11"] {
            std::fs::write(base.join(format!("{n}.png")), b"x").unwrap();
        }
        std::fs::write(base.join("2.zip"), b"x").unwrap();
        std::fs::create_dir_all(base.join("10_folder")).unwrap();
        std::fs::write(base.join("10_folder").join("a.webp"), b"x").unwrap();

        // Playlist order when opening the folder.
        let (src, _) = FsSource::open_folder(&base).unwrap();
        let names: Vec<String> = src
            .entries
            .iter()
            .map(|e| e.display_name.clone())
            .collect();
        assert_eq!(names, vec!["1.png", "2.png", "3.png", "11.png"]);

        // Sibling order for [ / ] (folder 10_ before file 11).
        let (items, _) = openable_siblings(&base.join("3.png"));
        let names: Vec<String> = items
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![
                "1.png",
                "2.png",
                "2.zip",
                "3.png",
                "10_folder",
                "11.png"
            ]
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
