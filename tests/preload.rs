//! Background full-preload: every source type must decode ALL entries via
//! `ImageSource::preload`, and streaming formats (tar.gz / 7z) must do it
//! in one front-to-back pass that maps members to their sorted indices.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use miru::source::{archive::open_source, DecodedImage, FsSource, ImageSource, SourceError};

type Emitted = Vec<(usize, Result<DecodedImage, SourceError>)>;

fn collect(source: &dyn ImageSource, limit: Option<usize>) -> Emitted {
    let mut out: Emitted = Vec::new();
    source.preload(&mut |index, result| {
        out.push((index, result));
        match limit {
            Some(n) => out.len() < n,
            None => true,
        }
    });
    out
}

fn png_bytes(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = RgbImage::from_pixel(w, h, Rgb([seed, 100, 200]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("encode png");
    buf
}

fn write_zip(path: &Path, members: &[(&str, Vec<u8>)]) {
    let file = fs::File::create(path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    for (name, data) in members {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("zip entry");
        zip.write_all(data).expect("zip write");
    }
    zip.finish().expect("zip finish");
}

fn write_targz(path: &Path, members: &[(&str, Vec<u8>)]) {
    let file = fs::File::create(path).expect("create tar.gz");
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    for (name, data) in members {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        tar.append_data(&mut header, name, data.as_slice())
            .expect("tar entry");
    }
    tar.into_inner()
        .expect("tar finish")
        .finish()
        .expect("gz finish");
}

/// Members in shuffled archive order with a non-image mixed in.
fn members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("10_0002.png", png_bytes(8, 8, 10)),
        ("docs/readme.txt", b"not an image".to_vec()),
        ("02_0001.png", png_bytes(12, 6, 2)),
        ("06_0005.png", png_bytes(6, 12, 6)),
    ]
}

fn expected_size(name: &str) -> (u32, u32) {
    match name {
        "02_0001.png" | "01_a.png" => (12, 6),
        "06_0005.png" | "02_b.png" => (6, 12),
        "10_0002.png" => (8, 8),
        other => panic!("unexpected entry {other}"),
    }
}

fn assert_preload_matches_entries(source: &dyn ImageSource, emitted: &Emitted) {
    let entries = source.entries();
    let mut by_index: HashMap<usize, (u32, u32)> = HashMap::new();
    assert_eq!(
        emitted.len(),
        entries.len(),
        "preload must emit exactly one result per entry"
    );
    for (index, result) in emitted {
        let img = result.as_ref().expect("preload decode ok");
        by_index.insert(*index, (img.width, img.height));
    }
    for (index, entry) in entries.iter().enumerate() {
        let (w, h) = by_index.get(&index).copied().expect("index emitted");
        assert_eq!(
            (w, h),
            expected_size(&entry.display_name),
            "wrong image at index {index}"
        );
    }
}

#[test]
fn zip_preloads_every_member() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.zip");
    write_zip(&path, &members());
    let (source, _) = open_source(&path).expect("open zip");
    let emitted = collect(source.as_ref(), None);
    assert_preload_matches_entries(source.as_ref(), &emitted);
}

#[test]
fn targz_preloads_every_member_in_one_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.tar.gz");
    write_targz(&path, &members());
    let (source, _) = open_source(&path).expect("open tar.gz");
    let emitted = collect(source.as_ref(), None);
    // Streaming pass: non-image member drained, images decoded to the
    // right sorted indices.
    assert_preload_matches_entries(source.as_ref(), &emitted);
}

#[test]
fn sevenz_preloads_every_member_in_one_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let stage = dir.path().join("stage");
    fs::create_dir(&stage).unwrap();
    for (name, data) in members() {
        let p = stage.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, data).unwrap();
    }
    let path: PathBuf = dir.path().join("pics.7z");
    sevenz_rust::compress_to_path(&stage, &path).expect("compress 7z");
    let (source, _) = open_source(&path).expect("open 7z");
    let emitted = collect(source.as_ref(), None);
    assert_preload_matches_entries(source.as_ref(), &emitted);
}

#[test]
fn folder_preloads_every_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("02_b.png"), png_bytes(6, 12, 2)).unwrap();
    fs::write(dir.path().join("01_a.png"), png_bytes(12, 6, 1)).unwrap();
    fs::write(dir.path().join("notes.txt"), b"skip me".to_vec()).unwrap();
    let (source, _) = FsSource::open_folder(dir.path()).expect("open folder");
    let emitted = collect(&source, None);
    assert_preload_matches_entries(&source, &emitted);
}

/// The emitter returning `false` must abort the pass (cancel plumbing).
#[test]
fn preload_aborts_when_emitter_stops() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.tar.gz");
    write_targz(&path, &members());
    let (source, _) = open_source(&path).expect("open tar.gz");
    let emitted = collect(source.as_ref(), Some(1));
    assert_eq!(emitted.len(), 1, "abort must stop after the first item");
    assert!(emitted[0].1.is_ok());
}
