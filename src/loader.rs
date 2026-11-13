use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crossbeam_channel::{Receiver, Sender, TryRecvError};

use crate::source::{DecodedImage, ImageSource, SourceError};

/// Job sent to decode workers (carries the source so workers stay generic).
struct LoadJob {
    index: usize,
    generation: u64,
    source: Arc<dyn ImageSource>,
    wanted: Arc<Wanted>,
    /// `true`: full-resolution zoom tier decode (`load_full`).
    full: bool,
}

/// Message back from workers.
pub enum LoaderMsg {
    Decoded {
        index: usize,
        generation: u64,
        image: DecodedImage,
    },
    /// Full-resolution decode result (zoom tier).
    DecodedFull {
        index: usize,
        generation: u64,
        image: DecodedImage,
    },
    Failed {
        index: usize,
        generation: u64,
        error: SourceError,
    },
    Skipped {
        index: usize,
        generation: u64,
    },
    /// Result of a background full-preload decode. Never touches
    /// `in_flight`; `poll` converts it into `Decoded`/`Failed`.
    Preloaded {
        index: usize,
        generation: u64,
        result: Result<DecodedImage, SourceError>,
    },
}

/// Indices the loader currently cares about (center + prefetch window of
/// the latest `request_around`). Shared with workers so queued-but-stale
/// jobs are skipped in microseconds instead of decoding ahead of the
/// freshly requested page — this is what makes big seekbar jumps fast.
#[derive(Default)]
struct Wanted(Mutex<HashSet<usize>>);

impl Wanted {
    fn replace(&self, indices: impl Iterator<Item = usize>) {
        let mut set = self.0.lock().unwrap();
        set.clear();
        set.extend(indices);
    }

    fn contains(&self, index: usize) -> bool {
        self.0.lock().unwrap().contains(&index)
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// Background decode pool (2 workers) + prefetch + dedupe.
/// Texture upload happens on the UI thread in `App::poll_loader`.
pub struct Loader {
    tx_job: Sender<LoadJob>,
    tx_msg: Sender<LoaderMsg>,
    rx_msg: Receiver<LoaderMsg>,
    source: Option<Arc<dyn ImageSource>>,
    generation: u64,
    in_flight: HashSet<usize>,
    full_in_flight: HashSet<usize>,
    wanted: Arc<Wanted>,
    /// Set when the source is replaced so the old preload thread stops.
    preload_cancel: Arc<AtomicBool>,
}

impl Loader {
    pub fn new() -> Self {
        let (tx_job, rx_job): (Sender<LoadJob>, Receiver<LoadJob>) = crossbeam_channel::unbounded();
        let (tx_msg, rx_msg) = crossbeam_channel::unbounded();
        let wanted: Arc<Wanted> = Arc::new(Wanted::default());
        // A couple more than the old fixed 2: decode is the bottleneck for
        // large pages, and prefetch jobs are cheap to skip when stale.
        let workers = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(2, 4);
        for _ in 0..workers {
            let rx_job = rx_job.clone();
            let tx_msg = tx_msg.clone();
            thread::spawn(move || {
                for job in rx_job {
                    let msg = if !job.wanted.contains(job.index) {
                        // Queued job superseded by a newer request.
                        LoaderMsg::Skipped {
                            index: job.index,
                            generation: job.generation,
                        }
                    } else {
                        let result = if job.full {
                            job.source.load_full(job.index)
                        } else {
                            job.source.load(job.index)
                        };
                        match result {
                            Ok(image) => {
                                if job.wanted.contains(job.index) {
                                    if job.full {
                                        LoaderMsg::DecodedFull {
                                            index: job.index,
                                            generation: job.generation,
                                            image,
                                        }
                                    } else {
                                        LoaderMsg::Decoded {
                                            index: job.index,
                                            generation: job.generation,
                                            image,
                                        }
                                    }
                                } else {
                                    // Superseded mid-decode: drop the image
                                    // so the UI thread never uploads it.
                                    LoaderMsg::Skipped {
                                        index: job.index,
                                        generation: job.generation,
                                    }
                                }
                            }
                            Err(error) => LoaderMsg::Failed {
                                index: job.index,
                                generation: job.generation,
                                error,
                            },
                        }
                    };
                    if tx_msg.send(msg).is_err() {
                        break;
                    }
                }
            });
        }
        Self {
            tx_job,
            tx_msg,
            rx_msg,
            source: None,
            generation: 0,
            in_flight: HashSet::new(),
            full_in_flight: HashSet::new(),
            wanted,
            preload_cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_source(&mut self, source: Arc<dyn ImageSource>) {
        self.generation = self.generation.wrapping_add(1);
        self.in_flight.clear();
        self.full_in_flight.clear();
        self.wanted.clear();
        while self.rx_msg.try_recv().is_ok() {}
        // Stop the previous preload pass, then start a fresh one: decode
        // every entry once in the background so page jumps hit the cache.
        self.preload_cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.preload_cancel = Arc::clone(&cancel);
        let tx = self.tx_msg.clone();
        let generation = self.generation;
        let preload_source = Arc::clone(&source);
        thread::spawn(move || {
            preload_source.preload(&mut |index, result| {
                if cancel.load(Ordering::Relaxed) {
                    return false;
                }
                tx.send(LoaderMsg::Preloaded {
                    index,
                    generation,
                    result,
                })
                .is_ok()
            });
        });
        self.source = Some(source);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Request the current index (high priority) + prefetch ±1/±2.
    pub fn request_around(&mut self, center: usize, len: usize) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if len == 0 {
            return;
        }
        let mut order = vec![center];
        for d in 1..=2usize {
            if center + d < len {
                order.push(center + d);
            }
            if center >= d {
                order.push(center - d);
            }
        }
        // Mark the wanted window FIRST: any queued job outside it is
        // skipped by workers the moment they pick it up.
        self.wanted.replace(order.iter().copied());
        for index in order {
            if index >= len || self.in_flight.contains(&index) {
                continue;
            }
            self.in_flight.insert(index);
            let _ = self.tx_job.try_send(LoadJob {
                index,
                generation: self.generation,
                source: source.clone(),
                wanted: Arc::clone(&self.wanted),
                full: false,
            });
        }
    }

    /// Request a full-resolution decode of `index` (zoom tier). Deduped
    /// while in flight; the UI caches the result in its full-res texture
    /// cache, so a completed decode is never re-requested.
    pub fn request_full(&mut self, index: usize, len: usize) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if len == 0 || index >= len || self.full_in_flight.contains(&index) {
            return;
        }
        // Keep the wanted window focused on the current page so stale
        // preview jobs stay skipped.
        self.wanted.replace([index].into_iter());
        self.full_in_flight.insert(index);
        let _ = self.tx_job.try_send(LoadJob {
            index,
            generation: self.generation,
            source,
            wanted: Arc::clone(&self.wanted),
            full: true,
        });
    }

    /// Non-blocking poll; returns ready messages for the current generation.
    pub fn poll(&mut self) -> Vec<LoaderMsg> {
        let mut out = Vec::new();
        loop {
            match self.rx_msg.try_recv() {
                Ok(msg) => {
                    match msg {
                        LoaderMsg::Preloaded {
                            index,
                            generation,
                            result,
                        } => {
                            // Background preload: no in_flight bookkeeping.
                            if generation == self.generation {
                                out.push(match result {
                                    Ok(image) => LoaderMsg::Decoded {
                                        index,
                                        generation,
                                        image,
                                    },
                                    Err(error) => LoaderMsg::Failed {
                                        index,
                                        generation,
                                        error,
                                    },
                                });
                            }
                        }
                        other => {
                            let (idx, gen) = match &other {
                                LoaderMsg::Decoded {
                                    index, generation, ..
                                }
                                | LoaderMsg::DecodedFull {
                                    index, generation, ..
                                }
                                | LoaderMsg::Failed {
                                    index, generation, ..
                                }
                                | LoaderMsg::Skipped {
                                    index, generation, ..
                                } => (*index, *generation),
                                LoaderMsg::Preloaded { .. } => unreachable!(),
                            };
                            self.in_flight.remove(&idx);
                            self.full_in_flight.remove(&idx);
                            if gen == self.generation && !matches!(other, LoaderMsg::Skipped { .. })
                            {
                                out.push(other);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

/// What `TextureCache::get` hands back: handle, stored dims, and the
/// original size when the entry is a downscaled preview.
pub type TextureLookup = (egui::TextureHandle, u32, u32, Option<(u32, u32)>);

/// LRU texture cache with a byte budget (default 256 MiB RGBA).
type CacheEntry = (egui::TextureHandle, usize, u32, u32, Option<(u32, u32)>);

/// LRU texture cache with a byte budget (default 256 MiB RGBA).
pub struct TextureCache {
    map: HashMap<usize, CacheEntry>,
    order: VecDeque<usize>,
    bytes: usize,
    pub budget: usize,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: 256 * 1024 * 1024,
        }
    }

    /// Returns `(handle, w, h, full_size)` where `full_size` is the
    /// original dimensions when this entry is a downscaled preview.
    pub fn get(&mut self, index: usize) -> Option<TextureLookup> {
        if let Some((tex, _len, w, h, full)) = self.map.get(&index) {
            let (tex, w, h, full) = (tex.clone(), *w, *h, *full);
            self.touch(index);
            Some((tex, w, h, full))
        } else {
            None
        }
    }

    pub fn insert(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        image: &DecodedImage,
        options: egui::TextureOptions,
    ) -> (egui::TextureHandle, u32, u32) {
        let name = format!("img-{index}");
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        let tex = ctx.load_texture(name, color, options);
        let len = image.byte_len();
        self.insert_handle(index, tex.clone(), len, image.width, image.height, image.full_size);
        (tex, image.width, image.height)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_handle(
        &mut self,
        index: usize,
        tex: egui::TextureHandle,
        len: usize,
        w: u32,
        h: u32,
        full_size: Option<(u32, u32)>,
    ) {
        if let Some((_, old_len, _, _, _)) = self.map.remove(&index) {
            self.bytes = self.bytes.saturating_sub(old_len);
            self.order.retain(|i| *i != index);
        }
        self.order.push_back(index);
        self.bytes += len;
        self.map.insert(index, (tex, len, w, h, full_size));
        self.evict_if_needed(None);
    }

    fn touch(&mut self, index: usize) {
        self.order.retain(|i| *i != index);
        self.order.push_back(index);
    }

    fn evict_if_needed(&mut self, protect: Option<usize>) {
        let mut guard = 0;
        while self.bytes > self.budget && self.order.len() > 1 && guard < 1024 {
            guard += 1;
            let candidate = *self.order.front().unwrap();
            if Some(candidate) == protect {
                self.order.pop_front();
                self.order.push_back(candidate);
                continue;
            }
        self.order.pop_front();
        if let Some((_, len, _, _, _)) = self.map.remove(&candidate) {
            self.bytes = self.bytes.saturating_sub(len);
        }
        }
    }

    pub fn protect(&mut self, index: usize) {
        self.touch(index);
        self.evict_if_needed(Some(index));
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }

    #[cfg(test)]
    pub fn test_insert_dummy(&mut self, ctx: &egui::Context, index: usize, len: usize) {
        let color = egui::ColorImage::filled([2, 2], egui::Color32::BLACK);
        let tex = ctx.load_texture(
            format!("test-{index}"),
            color,
            egui::TextureOptions::NEAREST,
        );
        self.insert_handle(index, tex, len, 2, 2, None);
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Entry;
    use std::time::{Duration, Instant};

    #[test]
    fn cache_evicts_oldest_over_budget() {
        let mut c = TextureCache::new();
        c.budget = 100;
        let ctx = egui::Context::default();
        c.test_insert_dummy(&ctx, 0, 60);
        c.test_insert_dummy(&ctx, 1, 60);
        assert!(!c.map.contains_key(&0));
        assert!(c.map.contains_key(&1));
    }

    #[test]
    fn cache_never_evicts_protected_current() {
        let mut c = TextureCache::new();
        c.budget = 100;
        let ctx = egui::Context::default();
        c.test_insert_dummy(&ctx, 0, 60);
        c.test_insert_dummy(&ctx, 1, 60);
        c.protect(1);
        assert!(c.map.contains_key(&1));
    }

    struct SlowSource {
        delay: Duration,
        loaded: Mutex<Vec<usize>>,
    }

    impl ImageSource for SlowSource {
        // Empty entry list => no background preload; this isolates the
        // wanted-window logic from preload interference.
        fn entries(&self) -> Vec<Entry> {
            vec![]
        }
        fn load(&self, index: usize) -> Result<DecodedImage, SourceError> {
            self.loaded.lock().unwrap().push(index);
            thread::sleep(self.delay);
            Ok(DecodedImage {
                rgba: vec![0; 4],
                width: 1,
                height: 1,
                full_size: None,
            })
        }
        fn source_label(&self) -> String {
            "slow".to_string()
        }
    }

    /// Source with a real entry list, so `set_source` starts a full preload.
    struct ListedSource {
        delay: Duration,
        entry_count: usize,
        loaded: Mutex<Vec<usize>>,
    }

    impl ImageSource for ListedSource {
        fn entries(&self) -> Vec<Entry> {
            (0..self.entry_count)
                .map(|i| Entry::file(std::path::PathBuf::from(format!("{i}.png"))))
                .collect()
        }
        fn load(&self, index: usize) -> Result<DecodedImage, SourceError> {
            self.loaded.lock().unwrap().push(index);
            thread::sleep(self.delay);
            Ok(DecodedImage {
                rgba: vec![0; 4],
                width: 1,
                height: 1,
                full_size: None,
            })
        }
        fn source_label(&self) -> String {
            "listed".to_string()
        }
    }

    /// Jumping to a new center must not decode jobs queued behind the old
    /// request: workers skip anything outside the latest wanted window.
    #[test]
    fn stale_queued_jobs_are_skipped_on_jump() {
        let mut loader = Loader::new();
        let src = Arc::new(SlowSource {
            delay: Duration::from_millis(40),
            loaded: Mutex::new(Vec::new()),
        });
        loader.set_source(Arc::clone(&src) as Arc<dyn ImageSource>);
        // Queues 28..=32; up to 4 workers start decoding, the last stays queued.
        loader.request_around(30, 100);
        // Jump far away before any worker frees up: the queued job must be
        // skipped, not decoded, and the new center must arrive.
        loader.request_around(50, 100);

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut got_center = false;
        while Instant::now() < deadline && !got_center {
            for msg in loader.poll() {
                if let LoaderMsg::Decoded { index, .. } = msg {
                    if index == 50 {
                        got_center = true;
                    }
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(got_center, "center job 50 never decoded");
        let loaded = src.loaded.lock().unwrap();
        assert!(
            !loaded.contains(&28),
            "stale queued job 28 was decoded: {loaded:?}"
        );
        assert!(loaded.contains(&50));
    }

    /// set_source starts a background preload that decodes every entry.
    #[test]
    fn preload_decodes_every_entry_in_background() {
        let mut loader = Loader::new();
        let src = Arc::new(ListedSource {
            delay: Duration::from_millis(1),
            entry_count: 6,
            loaded: Mutex::new(Vec::new()),
        });
        loader.set_source(Arc::clone(&src) as Arc<dyn ImageSource>);

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut seen = HashSet::new();
        while Instant::now() < deadline && seen.len() < 6 {
            for msg in loader.poll() {
                if let LoaderMsg::Decoded { index, .. } = msg {
                    seen.insert(index);
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(seen.len(), 6, "preloaded indices: {seen:?}");
        let loaded = src.loaded.lock().unwrap();
        assert_eq!(loaded.len(), 6, "preload loaded: {loaded:?}");
    }

    /// Replacing the source cancels the previous preload pass promptly.
    #[test]
    fn preload_is_cancelled_when_source_changes() {
        let mut loader = Loader::new();
        let src_a = Arc::new(ListedSource {
            delay: Duration::from_millis(40),
            entry_count: 100,
            loaded: Mutex::new(Vec::new()),
        });
        loader.set_source(Arc::clone(&src_a) as Arc<dyn ImageSource>);
        let src_b = Arc::new(ListedSource {
            delay: Duration::from_millis(0),
            entry_count: 3,
            loaded: Mutex::new(Vec::new()),
        });
        loader.set_source(Arc::clone(&src_b) as Arc<dyn ImageSource>);

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut seen = HashSet::new();
        while Instant::now() < deadline && seen.len() < 3 {
            for msg in loader.poll() {
                if let LoaderMsg::Decoded { index, .. } = msg {
                    seen.insert(index);
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(seen.len(), 3, "new source preloaded: {seen:?}");
        // The old pass is cancelled before its first emit: at most the one
        // item already mid-decode when the cancel flag flipped.
        let loaded_a = src_a.loaded.lock().unwrap();
        assert!(
            loaded_a.len() <= 1,
            "old preload kept running: {loaded_a:?}"
        );
        let loaded_b = src_b.loaded.lock().unwrap();
        assert_eq!(loaded_b.len(), 3);
    }
}
