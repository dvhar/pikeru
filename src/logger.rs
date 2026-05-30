//! Drop-in replacement for `env_logger` used by `portal.rs`.  Exports the
//! five standard macros (`trace!`, `debug!`, `info!`, `warn!`, `error!`) and
//! a `Builder` with the same API:
//!
//! ```ignore
//! use crate::logger::{Builder, LevelFilter};
//! Builder::new().filter_level(LevelFilter::Info).init();
//! ```
//!
//! All output goes to **stderr** as `[2026-05-29T23:46:02Z TRACE module_path] message`
//! (or `[TRACE module_path] message` if timestamps are disabled).

use std::sync::atomic::{AtomicBool, AtomicU8};


// ── LevelFilter ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LevelFilter {
    Off   = 0,
    Error = 1,
    Warn  = 2,
    Info  = 3,
    Debug = 4,
    Trace = 5,
}

impl std::fmt::Display for LevelFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelFilter::Off   => write!(f, "OFF"),
            LevelFilter::Error  => write!(f, "ERROR"),
            LevelFilter::Warn   => write!(f, "WARN"),
            LevelFilter::Info   => write!(f, "INFO"),
            LevelFilter::Debug  => write!(f, "DEBUG"),
            LevelFilter::Trace  => write!(f, "TRACE"),
        }
    }
}

// ── Global state (set once by `Builder::init`) ──────────────────────────

static GLOBAL_LEVEL: AtomicU8 = AtomicU8::new(LevelFilter::Info as u8);
static INITIALIZED: AtomicBool = AtomicBool::new(false);
pub(crate) static SHOW_TIMESTAMPS: AtomicBool = AtomicBool::new(true);


// ── Builder ─────────────────────────────────────────────────────────────

pub struct Builder {
    level:       LevelFilter,
    show_timestamps: bool,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            level:          LevelFilter::Info,
            show_timestamps: true,
        }
    }

    /// Set the global minimum log level.
    #[allow(dead_code)]
    pub fn filter_level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    /// Whether to include a timestamp prefix like `[2026-05-29T23:46:02Z TRACE]`.
    /// When disabled the format is `[TRACE module_path] message`.
    #[allow(dead_code)]
    pub fn show_timestamps(mut self, show: bool) -> Self {
        self.show_timestamps = show;
        self
    }

    /// Apply configuration.  Subsequent calls are no-ops.
    pub fn init(self) {
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        GLOBAL_LEVEL.store(self.level as u8, std::sync::atomic::Ordering::SeqCst);
        SHOW_TIMESTAMPS.store(self.show_timestamps, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Decide at runtime whether a message should be emitted.
pub(crate) fn should_log(_module_path: &str, level: LevelFilter) -> bool {
    level as u8 >= GLOBAL_LEVEL.load(std::sync::atomic::Ordering::SeqCst)
}

// ── The five exported macros ────────────────────────────────────────────
//
// Each one inlines the filter check so that no cross-module macro resolution
// is needed.  `#[macro_export]` puts them at the crate root, which means the
// calling code can use them as plain `info!()` etc.

/// Log at `Trace` level.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if $crate::logger::should_log(module_path!(), $crate::logger::LevelFilter::Trace) {
            let prefix = if $crate::logger::SHOW_TIMESTAMPS.load(std::sync::atomic::Ordering::SeqCst) {
                let dt: $crate::chrono::DateTime<$crate::chrono::Utc>
                    = $crate::chrono::DateTime::<$crate::chrono::Utc>::from(
                        SystemTime::now());
                format!("[{} TRACE] {} ", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"), module_path!())
            } else {
                format!("[TRACE] {} ", module_path!())
            };
            eprintln!("{}{}", prefix, format_args!($($arg)*));
        }
    }};
}

/// Log at `Debug` level.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        if $crate::logger::should_log(module_path!(), $crate::logger::LevelFilter::Debug) {
            let prefix = if $crate::logger::SHOW_TIMESTAMPS.load(std::sync::atomic::Ordering::SeqCst) {
                let dt: $crate::chrono::DateTime<$crate::chrono::Utc>
                    = $crate::chrono::DateTime::<$crate::chrono::Utc>::from(
                        SystemTime::now());
                format!("[{} DEBUG] {} ", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"), module_path!())
            } else {
                format!("[DEBUG] {} ", module_path!())
            };
            eprintln!("{}{}", prefix, format_args!($($arg)*));
        }
    }};
}

/// Log at `Info` level.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        if $crate::logger::should_log(module_path!(), $crate::logger::LevelFilter::Info) {
            let prefix = if $crate::logger::SHOW_TIMESTAMPS.load(std::sync::atomic::Ordering::SeqCst) {
                let dt: $crate::chrono::DateTime<$crate::chrono::Utc>
                    = $crate::chrono::DateTime::<$crate::chrono::Utc>::from(
                        SystemTime::now());
                format!("[{} INFO] {} ", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"), module_path!())
            } else {
                format!("[INFO] {} ", module_path!())
            };
            eprintln!("{}{}", prefix, format_args!($($arg)*));
        }
    }};
}

/// Log at `Warn` level.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        if $crate::logger::should_log(module_path!(), $crate::logger::LevelFilter::Warn) {
            let prefix = if $crate::logger::SHOW_TIMESTAMPS.load(std::sync::atomic::Ordering::SeqCst) {
                let dt: $crate::chrono::DateTime<$crate::chrono::Utc>
                    = $crate::chrono::DateTime::<$crate::chrono::Utc>::from(
                        SystemTime::now());
                format!("[{} WARN] {} ", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"), module_path!())
            } else {
                format!("[WARN] {} ", module_path!())
            };
            eprintln!("{}{}", prefix, format_args!($($arg)*));
        }
    }};
}

/// Log at `Error` level.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        if $crate::logger::should_log(module_path!(), $crate::logger::LevelFilter::Error) {
            let prefix = if $crate::logger::SHOW_TIMESTAMPS.load(std::sync::atomic::Ordering::SeqCst) {
                let dt: $crate::chrono::DateTime<$crate::chrono::Utc>
                    = $crate::chrono::DateTime::<$crate::chrono::Utc>::from(
                        SystemTime::now());
                format!("[{} ERROR] {} ", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"), module_path!())
            } else {
                format!("[ERROR] {} ", module_path!())
            };
            eprintln!("{}{}", prefix, format_args!($($arg)*));
        }
    }};
}
