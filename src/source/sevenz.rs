use std::path::Path;

use super::{is_image_member_name, SourceError};

pub fn list_members(path: &Path) -> Result<Vec<String>, SourceError> {
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| SourceError::Archive(format!("Invalid 7z '{}': {e:?}", path.display())))?;
    let mut names = Vec::new();
    reader
        .for_each_entries(|entry, _reader| {
            if entry.is_directory() {
                return Ok(true);
            }
            let name = entry.name().to_string();
            if is_image_member_name(&name) {
                names.push(name);
            }
            Ok(true)
        })
        .map_err(|e| SourceError::Archive(format!("Cannot list 7z '{}': {e:?}", path.display())))?;
    Ok(names)
}

pub fn extract_member(path: &Path, member: &str) -> Result<Vec<u8>, SourceError> {
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())
        .map_err(|e| SourceError::Archive(format!("Invalid 7z '{}': {e:?}", path.display())))?;
    let mut out: Option<Vec<u8>> = None;
    reader
        .for_each_entries(|entry, reader| {
            if entry.name() == member {
                let mut buf = Vec::with_capacity(entry.size().min(256 * 1024 * 1024) as usize);
                reader
                    .read_to_end(&mut buf)
                    .map_err(|e| sevenz_rust::Error::io_msg(e, "read 7z entry"))?;
                out = Some(buf);
                return Ok(false);
            }
            // Drain this entry's stream so solid blocks stay in sync.
            if !entry.is_directory() {
                let mut sink = std::io::sink();
                std::io::copy(reader, &mut sink)
                    .map_err(|e| sevenz_rust::Error::io_msg(e, "skip 7z entry"))?;
            }
            Ok(true)
        })
        .map_err(|e| SourceError::Archive(format!("Cannot extract '{member}': {e:?}")))?;
    out.ok_or_else(|| SourceError::Archive(format!("Missing entry '{member}'")))
}
