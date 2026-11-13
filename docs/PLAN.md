# view-image — Project Plan

A fast, dark-themed desktop image viewer with Honeyview-style chrome: auto-hiding
title bar and hotbar, next/previous navigation, zoom controls, and a seekbar over
the current folder or archive.

## Vision

Open a single image, a folder, or an archive (zip/cbz/rar/cbr/7z/tar.gz) and flip
through every image inside it with one click. The window stays out of the way:
all chrome auto-hides and the canvas is pure image.

## Scope

### v1 — in scope

- Open a standalone image file. Playlist is built from the parent folder and the
  opened file is the current entry (Honeyview behavior) — next/previous walks
  the siblings.
- Open a folder. All images in it (non-recursive v1) become the playlist.
- Open an archive. All image members become the playlist.
- Navigation: next/previous buttons, seekbar (position + drag-to-seek),
  keyboard arrows.
- Zoom in/out buttons, mouse wheel zoom, drag-to-pan, fit modes, rotation.
- Auto-hiding top bar (window buttons + menu row) and bottom hotbar.
- Custom window chrome: min/max/close, double-click titlebar to maximize,
  drag to move.
- Formats: PNG, JPEG, GIF (first frame v1), BMP, TIFF, WebP.
- Archives: `.zip`/`.cbz`, `.7z`, `.tar.gz`/`.tgz`, `.rar`/`.cbr`.
- Lock button: freeze zoom/pan/rotation across image changes.

### v1 — out of scope (later)

- Slideshow mode, bookmarks, editing/copy-to (menu entries may exist but can be
  disabled placeholders).
- Animated GIF/WebP playback, multi-page TIFF.
- Recursive folder listing, thumbnails panel, EXIF panel.
- Session persistence of last opened folder (M5 adds window-state persistence).

## Stack

| Concern            | Crate                                | Notes                                          |
|--------------------|--------------------------------------|------------------------------------------------|
| UI / window        | `eframe` / `egui`                    | Undecorated window, custom chrome, textures    |
| Image decoding     | `image` (webp via `image-webp`)      | png/jpeg/gif/bmp/tiff/webp                     |
| Zip / cbz          | `zip`                                | Read-only listing + per-entry extraction       |
| 7z                 | `sevenz-rust`                        | Pure Rust                                      |
| tar.gz / tgz       | `tar` + `flate2`                     | Pure Rust                                      |
| rar / cbr          | `unrar`                              | Bundled native unrar C++ lib (see Risks)       |
| Natural sort       | `natord`                             | `2_0001` before `10_0002` matters for playlists|
| Open dialog        | `rfd`                                | Native file/folder picker                      |
| Config persistence | `serde` + `toml`                     | M5 only                                        |

Edition 2024, Rust stable. No async runtime — background decode uses
`std::thread` + channels (see `docs/ARCHITECTURE.md`).

## Milestones

### M0 — Scaffolding and docs (done)

- [x] `cargo init` binary crate `view-image`
- [x] `docs/PLAN.md`, `docs/UI-SPEC.md`, `docs/ARCHITECTURE.md`
- [x] `.gitignore`: `/target`, `image-test/`

### M1 — Window and custom chrome

- Undecorated `eframe` window (`ViewportBuilder::with_decorations(false)`).
- Custom titlebar: app icon + title (left), min/max/close (right), all
  functional (winit viewport commands: minimize, maximize toggle, close).
- Drag-to-move (`window.drag_window()`), double-click titlebar = maximize toggle.
- Menu row (View / Slideshow / Bookmark / Edit / Copy to / Lock) as popovers;
  non-implemented entries shown disabled.
- Auto-hide engine for top bar and bottom hotbar (rules in `docs/UI-SPEC.md`).
- Dark theme + pink accent tokens.

### M2 — Loading, playlist, seekbar

- Open file / open folder / open archive (CLI arg, "Open" menu, drag-drop).
- `ImageSource` trait with `FsSource`; natural-sorted playlist.
- Next/previous buttons + arrow keys; seekbar showing `current/total`,
  click/drag to jump.
- Standalone file open → sibling playlist (jump to the opened file).
- First-fit image render on the dark canvas; empty-state placeholder.

### M3 — Zoom, rotation, hotbar buttons

- Zoom state: fit-window (default), fit-width, 1:1, free zoom (wheel + buttons).
- Drag-to-pan when zoomed; rotation 90° steps (CCW/CW buttons).
- Hotbar pill per screenshot: `[menu] [+] [−] [⟲] [⟳] [Lock] ‹ ›` with the
  seekbar strip above it.
- Lock toggle: keeps zoom/pan/rotation when changing images.

### M4 — Archive sources

- `ArchiveSource` with per-format adapters, order:
  1. zip/cbz (`zip` crate) — foundation of the entry model
  2. 7z (`sevenz-rust`)
  3. tar.gz/tgz (`tar` + `flate2`)
  4. rar/cbr (`unrar`) — isolated behind the adapter so it can be feature-flagged
- Archive entries listed, naturally sorted, seekbar works over the archive.
- Loading errors surfaced non-fatally (skip unreadable entries, log).

### M5 — Polish

- Decode preload + LRU texture cache (prefetch ±1/±2), fast prev/next.
- Full keyboard shortcut set (`docs/UI-SPEC.md`).
- Slideshow (interval, pause on hover/zoom).
- Window state persistence (size, position, last folder) via `serde`/`toml`.
- Perf pass on large images (decode downscale when image exceeds screen).

## Testing strategy

- **Manual corpus** — `image-test/` (git-ignored, supplied locally):
  - `01_cover_000.webp` — standalone file, sibling-playlist behavior
  - `Alt Account Trap!/` — 73 WebP files, natural order (`06_0005.webp` …
    `73_0072.webp`), tests seekbar + navigation at scale
  - `Alt Account Trap!.zip` — same content as folder, tests archive parity
- **Unit tests** (committed, no fixtures needed):
  - natural sort ordering, playlist cursor math (wrap, seek clamp),
    zoom/fit math, auto-hide timer logic, archive extension → format mapping.
- **Integration tests** — `tests/` with tiny generated PNG/WebP/zip fixtures
  committed under `tests/fixtures/` (kept small; `image-test/` stays ignored).
- **Checklist per milestone** — run through the corpus manually: open each
  source, navigate full range both directions, seekbar jumps, zoom/rotate/lock,
  chrome auto-hide on both edges, window buttons.

## Risks

| Risk                                       | Mitigation                                                        |
|--------------------------------------------|-------------------------------------------------------------------|
| `unrar` native lib build friction on Windows | Keep rar/cbr last in M4 behind a cargo feature; cbz/zip unaffected |
| Huge images blow memory / stutter           | Decode-time downscale to screen size, LRU cache with byte budget   |
| WebP decode speed in `image` crate          | Swap to `zune-webp`/`libwebp-sys` behind the loader if profiling demands |
| egui texture upload cost on rapid paging    | Prefetch + texture reuse; render current-first, prefetch in background |
| Auto-hide feels wrong (flicker, trapped)    | Explicit reveal/hide rules + pin toggle (Lock), tune timers in M1/M5 |
| Archive listing order differs from folder   | Always natural-sort entries by path regardless of archive order    |
