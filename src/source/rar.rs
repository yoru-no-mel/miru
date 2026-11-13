use std::path::Path;

use super::SourceError;

/// RAR adapter, feature-gated (`rar`). Kept isolated so `cbz`/zip flows
/// never touch the native unrar library.
#[cfg(feature = "rar")]
pub fn list_members(path: &Path) -> Result<Vec<String>, SourceError> {
    let archive = unrar::Archive::new(path)
        .open_for_listing()
        .map_err(|e| SourceError::Archive(format!("Invalid rar '{}': {e:?}", path.display())))?;
    let mut names = Vec::new();
    for item in archive {
        let header = item.map_err(|e| {
            SourceError::Archive(format!("Cannot list rar '{}': {e:?}", path.display()))
        })?;
        if header.is_directory() {
            continue;
        }
        let name = header.filename.to_string_lossy().replace('\\', "/");
        if super::is_image_member_name(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

#[cfg(feature = "rar")]
pub fn extract_member(path: &Path, member: &str) -> Result<Vec<u8>, SourceError> {
    let mut archive = unrar::Archive::new(path)
        .open_for_processing()
        .map_err(|e| SourceError::Archive(format!("Invalid rar '{}': {e:?}", path.display())))?;
    loop {
        let next = archive.read_header().map_err(|e| {
            SourceError::Archive(format!("Cannot read rar '{}': {e:?}", path.display()))
        })?;
        let Some(entry) = next else {
            break;
        };
        let name = entry.entry().filename.to_string_lossy().replace('\\', "/");
        if name == member {
            let (data, _rest) = entry
                .read()
                .map_err(|e| SourceError::Archive(format!("Cannot extract '{member}': {e:?}")))?;
            return Ok(data);
        } else {
            archive = entry.skip().map_err(|e| {
                SourceError::Archive(format!("Cannot read rar '{}': {e:?}", path.display()))
            })?;
        }
    }
    Err(SourceError::Archive(format!("Missing entry '{member}'")))
}

#[cfg(not(feature = "rar"))]
pub fn list_members(path: &Path) -> Result<Vec<String>, SourceError> {
    Err(SourceError::Unsupported(format!(
        "RAR support disabled (build with --features rar): '{}'",
        path.display()
    )))
}

#[cfg(not(feature = "rar"))]
pub fn extract_member(path: &Path, member: &str) -> Result<Vec<u8>, SourceError> {
    Err(SourceError::Unsupported(format!(
        "RAR support disabled (build with --features rar): '{}::{member}'",
        path.display()
    )))
}
