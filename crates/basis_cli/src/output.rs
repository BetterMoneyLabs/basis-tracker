//! Output mode handling for human vs JSON rendering.
//!
//! When `--json` is passed, every command prints a single JSON document to
//! stdout. Progress/diagnostic lines (which are part of the human output) are
//! routed to stderr in JSON mode so stdout stays machine-parseable.

use std::sync::atomic::{AtomicBool, Ordering};

static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// Enable or disable JSON output mode (set once at startup from the --json flag).
pub fn set_json_mode(json: bool) {
    JSON_MODE.store(json, Ordering::Relaxed);
}

/// Whether JSON output mode is active.
pub fn json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

/// Print a progress/diagnostic line. In JSON mode the line goes to stderr so
/// that stdout carries only the final JSON document; in human mode it behaves
/// exactly like `println!`.
macro_rules! progress {
    ($($arg:tt)*) => {{
        if $crate::output::json_mode() {
            eprintln!($($arg)*);
        } else {
            println!($($arg)*);
        }
    }};
}

pub(crate) use progress;
