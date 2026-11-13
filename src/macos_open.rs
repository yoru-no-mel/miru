//! Receives macOS LaunchServices "open document" Apple Events.
//!
//! File drops are reported by winit, but files selected through Finder's
//! Open With menu arrive as an Apple Event instead. Keep the native event
//! handler small and hand paths back to the normal app update loop.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static OPENED_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

unsafe extern "C" {
    fn miru_register_open_handler();
}

pub fn register() {
    // SAFETY: the Objective-C helper registers a process-lifetime handler
    // with NSAppleEventManager; it does not retain any Rust references.
    unsafe { miru_register_open_handler() };
}

pub fn take_first() -> Option<PathBuf> {
    let mut paths = OPENED_PATHS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *paths).into_iter().next()
}

/// Called by the Objective-C Apple Event handler for each received file URL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn miru_open_document(path: *const c_char) {
    if path.is_null() {
        return;
    }

    // SAFETY: the native callback passes a NUL-terminated filesystem path for
    // the duration of this call.
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .into_owned();
    OPENED_PATHS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(PathBuf::from(path));
}
