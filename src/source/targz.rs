use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::{is_image_member_name, SourceError};

pub fn list_members(path: &Path) -> Result<Vec<String>, SourceError> {
    let file = File::open(path)
        .map_err(|e| SourceError::Io(format!("Cannot open '{}': {e}", path.display())))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let mut names = Vec::new();
    let entries = archive
        .entries()
        .map_err(|e| SourceError::Archive(format!("Invalid tar.gz '{}': {e}", path.display())))?;
    for item in entries {
        let item = item.map_err(|e| SourceError::Archive(format!("{e}")))?;
        if item.header().entry_type().is_dir() {
            continue;
        }
        let p = item
            .path()
            .map_err(|e| SourceError::Archive(format!("{e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        if is_image_member_name(&p) {
            names.push(p);
        }
    }
    Ok(names)
}

pub fn extract_member(path: &Path, member: &str) -> Result<Vec<u8>, SourceError> {
    let file = File::open(path)
        .map_err(|e| SourceError::Io(format!("Cannot open '{}': {e}", path.display())))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let entries = archive
        .entries()
        .map_err(|e| SourceError::Archive(format!("Invalid tar.gz '{}': {e}", path.display())))?;
    for item in entries {
        let mut item = item.map_err(|e| SourceError::Archive(format!("{e}")))?;
        let p = item
            .path()
            .map_err(|e| SourceError::Archive(format!("{e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        if p == member {
            let mut buf = Vec::with_capacity(item.size().min(64 * 1024 * 1024) as usize);
            item.read_to_end(&mut buf)
                .map_err(|e| SourceError::Archive(format!("Cannot extract '{member}': {e}")))?;
            return Ok(buf);
        }
    }
    Err(SourceError::Archive(format!("Missing entry '{member}'")))
}
