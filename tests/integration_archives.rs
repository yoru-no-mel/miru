//! Archive sources: zip/cbz, tar.gz/tgz, 7z roundtrips with generated fixtures.
//! RAR is covered by the same `ArchiveSource` path behind the `rar` feature
//! (no pure-Rust writer available for fixtures, verified manually).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use miru::source::{archive::open_source, ImageSource};

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

fn members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("10_0002.png", png_bytes(8, 8, 10)),
        ("docs/readme.txt", b"not an image".to_vec()),
        ("02_0001.png", png_bytes(12, 6, 2)),
        ("06_0005.png", png_bytes(6, 12, 6)),
    ]
}

fn check_sorted_image_listing(source: &dyn ImageSource) {
    let names: Vec<_> = source
        .entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(names, vec!["02_0001.png", "06_0005.png", "10_0002.png"]);
}

#[test]
fn zip_lists_and_decodes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.zip");
    write_zip(&path, &members());
    let (source, cursor) = open_source(&path).expect("open zip");
    assert_eq!(cursor, 0);
    check_sorted_image_listing(source.as_ref());
    let img = source.load(0).expect("decode from zip");
    assert_eq!((img.width, img.height), (12, 6));
}

#[test]
fn cbz_uses_zip_adapter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("comic.cbz");
    write_zip(&path, &members());
    let (source, _) = open_source(&path).expect("open cbz");
    check_sorted_image_listing(source.as_ref());
}

#[test]
fn targz_lists_and_decodes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.tar.gz");
    write_targz(&path, &members());
    let (source, _) = open_source(&path).expect("open tar.gz");
    check_sorted_image_listing(source.as_ref());
    let img = source.load(2).expect("decode from tar.gz");
    assert_eq!((img.width, img.height), (8, 8));
}

#[test]
fn tgz_extension_maps_to_targz() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pics.tgz");
    write_targz(&path, &members());
    let (source, _) = open_source(&path).expect("open tgz");
    check_sorted_image_listing(source.as_ref());
}

#[test]
fn sevenz_lists_and_decodes() {
    let dir = tempfile::tempdir().expect("tempdir");
    // sevenz-rust compresses a directory; stage members as files.
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
    let names: Vec<_> = source
        .entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    // Member paths include the stage dir prefix; image set must match.
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"02_0001.png".to_string()));
    assert!(names.contains(&"06_0005.png".to_string()));
    assert!(names.contains(&"10_0002.png".to_string()));
    let img = source.load(0).expect("decode from 7z");
    assert!(!img.rgba.is_empty());
}

#[test]
fn zip_without_images_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.zip");
    write_zip(&path, &[("readme.txt", b"hi".to_vec())]);
    assert!(open_source(&path).is_err());
}
