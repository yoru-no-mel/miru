use std::fs::File;
use std::path::Path;

use super::{is_image_member_name, SourceError};

pub fn list_members(path: &Path) -> Result<Vec<String>, SourceError> {
    let file = File::open(path)
        .map_err(|e| SourceError::Io(format!("Cannot open '{}': {e}", path.display())))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SourceError::Archive(format!("Invalid zip '{}': {e}", path.display())))?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| SourceError::Archive(format!("{e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if is_image_member_name(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

pub fn extract_member(path: &Path, member: &str) -> Result<Vec<u8>, SourceError> {
    let file = File::open(path)
        .map_err(|e| SourceError::Io(format!("Cannot open '{}': {e}", path.display())))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SourceError::Archive(format!("Invalid zip '{}': {e}", path.display())))?;
    let mut entry = archive
        .by_name(member)
        .map_err(|e| SourceError::Archive(format!("Missing entry '{member}': {e}")))?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    std::io::Read::read_to_end(&mut entry, &mut buf)
        .map_err(|e| SourceError::Archive(format!("Cannot extract '{member}': {e}")))?;
    Ok(buf)
}
