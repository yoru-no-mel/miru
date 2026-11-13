//! Filesystem sources: folder playlists and single-file sibling behavior.
//! Fixtures are generated at test time (tiny PNG/WebP), nothing committed.

use std::fs;
use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use view_image::playlist::Playlist;
use view_image::source::archive::open_source;

fn write_png(path: &Path, w: u32, h: u32, color: Rgb<u8>) {
    let img = RgbImage::from_pixel(w, h, color);
    img.save(path).expect("write png");
}

fn write_webp(path: &Path, w: u32, h: u32) {
    let img = RgbImage::from_pixel(w, h, Rgb([10, 20, 30]));
    let file = fs::File::create(path).expect("create webp");
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
    encoder
        .encode(&img.into_raw(), w, h, image::ExtendedColorType::Rgb8)
        .expect("write webp");
}

fn fixture_dir(names: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    for (i, name) in names.iter().enumerate() {
        let p = dir.path().join(name);
        let c = (i as u8).wrapping_mul(40);
        if name.ends_with(".webp") {
            write_webp(&p, 8, 8);
        } else if name.ends_with(".png") {
            write_png(&p, 8 + i as u32, 8, Rgb([c, 100, 200]));
        } else {
            fs::write(&p, "not an image").expect("write text");
        }
    }
    let path = dir.path().to_path_buf();
    (dir, path)
}

#[test]
fn folder_lists_images_sorted_naturally() {
    let (_tmp, dir) = fixture_dir(&["10_0002.png", "02_0001.png", "note.txt", "06_0005.png"]);
    let (source, cursor) = open_source(&dir).expect("open folder");
    assert_eq!(cursor, 0);
    let names: Vec<_> = source
        .entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(names, vec!["02_0001.png", "06_0005.png", "10_0002.png"]);
    assert_eq!(source.entries().len(), 3, "non-images excluded");
}

#[test]
fn single_file_opens_sibling_playlist_at_file() {
    let (_tmp, dir) = fixture_dir(&["b.png", "a.png", "c.png"]);
    let (source, cursor) = open_source(&dir.join("b.png")).expect("open file");
    let names: Vec<_> = source
        .entries()
        .iter()
        .map(|e| e.display_name.clone())
        .collect();
    assert_eq!(names, vec!["a.png", "b.png", "c.png"]);
    assert_eq!(cursor, 1, "cursor jumps to the opened file");

    let mut playlist = Playlist::new(source.entries(), cursor);
    assert_eq!(playlist.current().unwrap().display_name, "b.png");
    assert!(playlist.next());
    assert_eq!(playlist.current().unwrap().display_name, "c.png");
    assert!(!playlist.next(), "no wrap in v1");
    assert!(playlist.prev());
    assert!(playlist.prev());
    assert!(!playlist.prev());
}

#[test]
fn fs_load_decodes_png_and_webp() {
    let (_tmp, dir) = fixture_dir(&["a.png", "z.webp"]);
    let (source, _) = open_source(&dir).expect("open folder");
    // Sorted: a.png first, z.webp second.
    let img0 = source.load(0).expect("decode png");
    assert_eq!((img0.width, img0.height), (8, 8));
    assert_eq!(img0.rgba.len(), 8 * 8 * 4);
    let img1 = source.load(1).expect("decode webp");
    assert_eq!((img1.width, img1.height), (8, 8));
}

#[test]
fn empty_folder_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(open_source(dir.path()).is_err());
}

#[test]
fn unsupported_file_is_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("readme.md");
    fs::write(&p, "hello").unwrap();
    assert!(open_source(&p).is_err());
}

#[test]
fn missing_path_is_an_error() {
    let missing = PathBuf::from("definitely-not-here-12345.png");
    assert!(open_source(&missing).is_err());
}
