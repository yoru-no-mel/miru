use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::fs::FsSource;
use super::{
    archive_format_for_path, decode_bytes, decode_bytes_full, is_image_member_name,
    sort_entries_naturally, ArchiveFormat, DecodedImage, Entry, ImageSource, SourceError,
};

mod zip_adapter {
    pub use crate::source::zip::{extract_member, list_members};
}
mod sevenz_adapter {
    pub use crate::source::sevenz::{extract_member, list_members};
}
mod targz_adapter {
    pub use crate::source::targz::{extract_member, list_members};
}
mod rar_adapter {
    pub use crate::source::rar::{extract_member, list_members};
}

/// Archive-backed image source. Member list is read once at open time;
/// every `load()` re-opens the archive so the source stays `Send + Sync`
/// and decode workers never share handles.
pub struct ArchiveSource {
    path: PathBuf,
    format: ArchiveFormat,
    members: Vec<String>,
    label: String,
}

impl ArchiveSource {
    pub fn open(path: &Path) -> Result<(Self, usize), SourceError> {
        let format = archive_format_for_path(path).ok_or_else(|| {
            SourceError::Unsupported(format!("Not an archive: '{}'", path.display()))
        })?;
        let members = list_members(path, format)?;
        if members.is_empty() {
            return Err(SourceError::Unsupported(format!(
                "No images found in archive '{}'",
                path.display()
            )));
        }
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("archive")
            .to_string();
        Ok((
            Self {
                path: path.to_path_buf(),
                format,
                members,
                label,
            },
            0,
        ))
    }
}

fn list_members(path: &Path, format: ArchiveFormat) -> Result<Vec<String>, SourceError> {
    let mut members = match format {
        ArchiveFormat::Zip => zip_adapter::list_members(path)?,
        ArchiveFormat::SevenZ => sevenz_adapter::list_members(path)?,
        ArchiveFormat::TarGz => targz_adapter::list_members(path)?,
        ArchiveFormat::Rar => rar_adapter::list_members(path)?,
    };
    members.sort_by(|a, b| natord::compare_ignore_case(a, b).then_with(|| natord::compare(a, b)));
    Ok(members)
}

fn extract_member(
    path: &Path,
    format: ArchiveFormat,
    member: &str,
) -> Result<Vec<u8>, SourceError> {
    match format {
        ArchiveFormat::Zip => zip_adapter::extract_member(path, member),
        ArchiveFormat::SevenZ => sevenz_adapter::extract_member(path, member),
        ArchiveFormat::TarGz => targz_adapter::extract_member(path, member),
        ArchiveFormat::Rar => rar_adapter::extract_member(path, member),
    }
}

impl ArchiveSource {
    /// Single front-to-back pass for solid/streaming formats. A per-index
    /// `load()` re-decompresses the whole prefix each time (O(n²)); one
    /// ordered pass decodes every image instead.
    fn preload_targz(
        &self,
        emit: &mut dyn FnMut(usize, Result<DecodedImage, SourceError>) -> bool,
    ) {
        let index_of: HashMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.as_str(), i))
            .collect();
        let Ok(file) = File::open(&self.path) else {
            return;
        };
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        let Ok(entries) = archive.entries() else {
            return;
        };
        for item in entries {
            let Ok(mut item) = item else {
                continue;
            };
            if item.header().entry_type().is_dir() {
                continue;
            }
            let Ok(p) = item.path() else {
                continue;
            };
            let name = p.to_string_lossy().replace('\\', "/");
            drop(p);
            if !is_image_member_name(&name) {
                continue;
            }
            let Some(&index) = index_of.get(name.as_str()) else {
                continue;
            };
            let mut buf = Vec::with_capacity(item.size().min(256 * 1024 * 1024) as usize);
            let result = match item.read_to_end(&mut buf) {
                Ok(_) => decode_bytes(&buf),
                Err(e) => Err(SourceError::Archive(format!("Cannot extract '{name}': {e}"))),
            };
            if !emit(index, result) {
                return;
            }
        }
    }

    fn preload_sevenz(
        &self,
        emit: &mut dyn FnMut(usize, Result<DecodedImage, SourceError>) -> bool,
    ) {
        let index_of: HashMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.as_str(), i))
            .collect();
        let Ok(mut reader) =
            sevenz_rust::SevenZReader::open(&self.path, sevenz_rust::Password::empty())
        else {
            return;
        };
        let _ = reader.for_each_entries(|entry, reader| {
            if entry.is_directory() {
                return Ok(true);
            }
            let name = entry.name().to_string();
            let index = index_of.get(name.as_str()).copied();
            if !is_image_member_name(&name) || index.is_none() {
                // Drain this entry's stream so solid blocks stay in sync.
                let mut sink = std::io::sink();
                let _ = std::io::copy(reader, &mut sink);
                return Ok(true);
            }
            let mut buf = Vec::with_capacity(entry.size().min(256 * 1024 * 1024) as usize);
            let result = match reader.read_to_end(&mut buf) {
                Ok(_) => decode_bytes(&buf),
                Err(e) => Err(SourceError::Archive(format!("Cannot extract '{name}': {e:?}"))),
            };
            if emit(index.unwrap(), result) {
                Ok(true)
            } else {
                Ok(false)
            }
        });
    }
}

impl ImageSource for ArchiveSource {
    fn entries(&self) -> Vec<Entry> {
        self.members
            .iter()
            .map(|m| Entry::archive_member(self.path.clone(), m.clone()))
            .collect()
    }

    fn load(&self, index: usize) -> Result<DecodedImage, SourceError> {
        let bytes = self.extract(index)?;
        decode_bytes(&bytes)
    }

    fn load_full(&self, index: usize) -> Result<DecodedImage, SourceError> {
        let bytes = self.extract(index)?;
        decode_bytes_full(&bytes)
    }

    fn preload(
        &self,
        emit: &mut dyn FnMut(usize, Result<DecodedImage, SourceError>) -> bool,
    ) {
        // Zip/Rar members are seekable: plain sequential loads are cheap.
        // Tar.gz / 7z are streamed front-to-back, so use the single pass.
        match self.format {
            ArchiveFormat::Zip | ArchiveFormat::Rar => {
                for index in 0..self.members.len() {
                    if !emit(index, self.load(index)) {
                        return;
                    }
                }
            }
            ArchiveFormat::TarGz => self.preload_targz(emit),
            ArchiveFormat::SevenZ => self.preload_sevenz(emit),
        }
    }

    fn source_label(&self) -> String {
        self.label.clone()
    }
}

impl ArchiveSource {
    fn extract(&self, index: usize) -> Result<Vec<u8>, SourceError> {
        let member = self
            .members
            .get(index)
            .ok_or_else(|| SourceError::Unsupported(format!("Index {index} out of range")))?;
        extract_member(&self.path, self.format, member)
    }
}

/// Open any supported path: directory -> folder source, archive ->
/// archive source, image file -> sibling playlist. Returns the source
/// plus the initial cursor.
pub fn open_source(path: &Path) -> Result<(Box<dyn ImageSource>, usize), SourceError> {
    if path.is_dir() {
        let (src, cursor) = FsSource::open_folder(path)?;
        return Ok((Box::new(src), cursor));
    }
    if archive_format_for_path(path).is_some() {
        if !path.exists() {
            return Err(SourceError::Io(format!(
                "File not found: '{}'",
                path.display()
            )));
        }
        let (src, cursor) = ArchiveSource::open(path)?;
        return Ok((Box::new(src), cursor));
    }
    if super::is_image_path(path) {
        if !path.exists() {
            return Err(SourceError::Io(format!(
                "File not found: '{}'",
                path.display()
            )));
        }
        let (src, cursor) = FsSource::open_file(path)?;
        return Ok((Box::new(src), cursor));
    }
    // Non-existent path that looks like nothing: friendly error.
    if !path.exists() {
        return Err(SourceError::Io(format!(
            "File not found: '{}'",
            path.display()
        )));
    }
    Err(SourceError::Unsupported(format!(
        "Unsupported file type: '{}'",
        path.display()
    )))
}

/// Build entries for tests / playlist construction, sorted naturally.
pub fn sorted_file_entries_for_test(names: &[&str]) -> Vec<Entry> {
    let mut entries: Vec<Entry> = names
        .iter()
        .map(|n| Entry::file(PathBuf::from(n)))
        .collect();
    sort_entries_naturally(&mut entries);
    entries
}
