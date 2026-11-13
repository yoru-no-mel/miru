# miru — Architecture

Single binary crate, no async runtime. UI runs on the main thread (egui);
decoding runs on worker threads and hands results back through a channel.

## Module tree

```
src/
├── main.rs            # eframe setup: undecorated viewport, fonts, theme
├── app.rs             # App struct: owns all state, wires UI + loader
├── playlist.rs        # Playlist: entries + cursor, navigation/seek math
├── source/
│   ├── mod.rs         # ImageSource trait, Entry, source factory (by path/extension)
│   ├── fs.rs          # FsSource: single file (sibling playlist) or folder
│   └── archive.rs     # ArchiveSource + per-format adapters
│       ├── zip.rs     #   zip / cbz      (zip crate)
│       ├── sevenz.rs  #   7z             (sevenz-rust)
│       ├── targz.rs   #   tar.gz / tgz   (tar + flate2)
│       └── rar.rs     #   rar / cbr      (unrar; feature-gated)
├── loader.rs          # decode worker pool, prefetch, LRU texture cache
├── viewer.rs          # image widget: ZoomState, fit modes, rotation, pan
├── chrome/
│   ├── mod.rs         # ChromeState: visibility timers, reveal/hide, pin
│   ├── titlebar.rs    # drag, double-click maximize, min/max/close buttons
│   ├── menu.rs        # menu row + popovers
│   ├── hotbar.rs      # floating pill: buttons + lock toggle
│   └── seekbar.rs     # track, thumb, drag-to-seek, tooltip
├── input.rs           # keyboard shortcuts, wheel zoom, drag pan, edge zones
└── theme.rs           # color tokens (see UI-SPEC), style setup
```

## Core types

```rust
// source/mod.rs
pub struct Entry {
    pub display_name: String,     // e.g. "06_0005.webp"
    pub sort_key: String,         // naturally sorted (natord)
    pub locator: Locator,
}

pub enum Locator {
    File(PathBuf),                          // filesystem path
    ArchiveMember { archive: PathBuf, index: usize },
}

pub trait ImageSource {
    /// Natural-sorted image entries. Called once at open time.
    fn entries(&self) -> Result<Vec<Entry>, SourceError>;
    /// Decode entry at `index` to RGBA8 + dimensions.
    fn load(&self, index: usize) -> Result<DecodedImage, SourceError>;
}

pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// playlist.rs
pub struct Playlist {
    entries: Vec<Entry>,
    cursor: usize,
}
// next()/prev()/seek(idx)/current() — cursor clamps, no wrap in v1

// viewer.rs
pub enum FitMode { FitWindow, FitWidth, ActualSize, Free }

pub struct ZoomState {
    pub scale: f32,
    pub offset: Vec2,      // pan offset in screen px
    pub fit_mode: FitMode,
    pub rotation: i8,      // quarter turns: -2..=2, normalized
}

// chrome/mod.rs
pub struct ChromeState {
    top_visible: bool,
    bottom_visible: bool,
    last_activity: Instant,
    pinned: bool,                 // Lock toggle / Pin UI
    active_interaction: bool,     // drag/menu open — suspends hiding
}

// loader.rs
pub enum LoaderMsg {
    Decoded { index: usize, image: DecodedImage },
    Failed { index: usize, error: String },
}
```

## Data flow

```
open(path)
  │
  ├─ source::factory(path) -> Box<dyn ImageSource>      (file | folder | archive)
  ├─ source.entries() -> Vec<Entry>  (natural sort)     -> Playlist
  ├─ playlist.seek(opened entry)                        (sibling jump for files)
  │
  └─ loader.request(current, priority=High)
        │  (also prefetch current±1, ±2 at Low priority)
        ▼
   decode worker thread pool (std::thread + crossbeam-style channel)
        │  LoaderMsg::Decoded { index, rgba }
        ▼
   app polls channel in update():
        ├─ texture_cache.insert(index, egui texture)    (LRU, byte budget)
        └─ viewer renders texture for playlist.cursor
```

- **Listing** is synchronous at open time (archives: read central directory /
  headers only, no full extract).
- **Decoding** is async off-thread; the UI never blocks on a decode. While the
  current index is not yet decoded, the previous frame (or a spinner) stays.
- **Texture upload** happens on the UI thread from `update()` after the channel
  delivers `DecodedImage` (egui textures must be created on the UI thread).

## Loader / cache policy

- Worker pool: 2 threads (v1), one job in flight per index (dedupe requests).
- `TextureCache`: `HashMap<usize, CachedTexture>` + LRU order; byte budget
  default 256 MiB of RGBA data; evict least-recently-viewed first, never evict
  the current index while displayed.
- Prefetch window: `current-2..=current+2`, Low priority; cancel stale prefetch
  jobs when the cursor jumps (seekbar drag) — decode jobs check a generation
  counter before uploading.
- Decode-time downscale: if `max(width, height) > 8192` (or exceeds 4× screen
  dimension), decode via the `image` crate's downscale-on-load path.

## Source layer details

- **FsSource**
  - File input: `entries()` = natural-sorted image files in the parent
    directory (non-recursive); `cursor` initialized to the opened file.
  - Folder input: natural-sorted image files in the folder (non-recursive).
  - Image filter: extension in `{png, jpg, jpeg, gif, bmp, tif, tiff, webp}`.
- **ArchiveSource**
  - Wraps a format adapter trait:

    ```rust
    trait ArchiveAdapter {
        fn list(&mut self) -> Result<Vec<String>, SourceError>; // member names
        fn extract(&mut self, index: usize) -> Result<Vec<u8>, SourceError>;
    }
    ```

  - `entries()` = adapter list filtered to image extensions, natural sort by
    member path (never rely on archive order).
  - `load()` = adapter extract → `image::load_from_memory` → RGBA8.
  - Adapter selected by extension (`.cbz` → zip adapter, `.cbr` → rar adapter).
- **Factory** (`source::mod.rs`): `open(path)` matches — directory → `FsSource`,
  extension in archive set → `ArchiveSource`, known image extension → `FsSource`
  (file mode), else error dialog.

## Chrome / auto-hide engine

`ChromeState::update(ctx, pointer_pos, window_rect)` each frame:

1. Reveal zones: `pointer.y <= 48` → show top; `pointer.y >= height-48` → show
   bottom; any pointer motion while hidden → show both (one-shot).
2. Start/refresh idle timer on pointer motion or input events.
3. Hide a panel when `now - last_activity > 1.5s` && pointer outside panel rect
   && !pinned && !active_interaction.
4. Panels animate opacity/offset; visibility is logical (rect is excluded from
   hit-testing when hidden).

Window commands used by titlebar (via `egui::ViewportCommand`):
`Minimize`, `Maximized(bool)`, `Close`, `StartDrag`, `Fullscreen`.

## Threading summary

| Thread            | Work                                                       |
|-------------------|------------------------------------------------------------|
| Main (UI)         | egui frame, input, chrome logic, texture create, rendering |
| Decode workers ×2 | archive extract (mem), `image` decode, downscale, RGBA     |

Communication: `std::sync::mpsc` (or `crossbeam-channel` if needed) both ways —
`loader` receives `LoadRequest { index, generation }`, sends `LoaderMsg`.
No locks on the hot path except the cache map (behind `Mutex`, held briefly).

## Error handling

- `SourceError` (IO, unsupported format, corrupt entry) → surfaced in a toast /
  status line in the hotbar area; unreadable archive entries are skipped with a
  warning so the rest of the playlist still works.
- Loader failures for non-current indices are logged and retried once on next
  navigation to that index.
- Fatal: window/renderer init failure → exit with message on stderr.
