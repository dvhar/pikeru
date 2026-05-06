//! XDG icon theme lookup using linicon.
//!
//! Resolves standard icon names (e.g. "folder", "audio-x-generic") to actual
//! image files on disk, falling back to bundled assets when no system icon is found.
use linicon::{IconType, lookup_icon};
use std::path::PathBuf;

/// Quality score for ranking icon results.
///
/// SVG always wins (infinite resolution). PNG is scored by min_size
/// (higher resolution = better quality). XPM is lowest priority.
fn icon_quality(icon_type: &IconType, min_size: u16) -> u32 {
    match icon_type {
        IconType::SVG => u32::MAX,
        IconType::PNG => min_size as u32,
        IconType::XMP => 1,
    }
}

/// Resolves an icon name to the *highest quality* available icon.
///
/// Collects all matches from `linicon::lookup_icon` and selects the best one:
/// - SVG always wins (scalable to any resolution)
/// - Among PNGs, the one with the largest `min_size` is preferred
/// - XPM is a last resort
///
/// Returns `Some(path, icon_type)` if a matching icon is found, or `None`.
pub fn get_icon_path(icon_name: &str, theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    let mut iter = lookup_icon(icon_name);
    if let Some(t) = theme_name {
        iter = iter.from_theme(t);
    }

    let mut best: Option<(PathBuf, IconType, u32)> = None;

    for result in iter {
        if let Ok(icon_path) = result {
            let quality = icon_quality(&icon_path.icon_type, icon_path.min_size);
            match &best {
                None => best = Some((icon_path.path, icon_path.icon_type, quality)),
                Some((_, _, best_quality)) => {
                    if quality > *best_quality {
                        best = Some((icon_path.path, icon_path.icon_type, quality));
                    }
                }
            }
        }
    }

    best.map(|(path, icon_type, _)| (path, icon_type))
}

/// Specific helpers for common icons used by pikeru.
pub fn get_folder_icon_path(theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    get_icon_path("folder", theme_name)
}

pub fn get_document_icon_path(theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    get_icon_path("text-x-generic", theme_name)
        .or_else(|| get_icon_path("x-office-document", theme_name))
        .or_else(|| get_icon_path("application-x-executable", theme_name))
}

pub fn get_unknown_icon_path(theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    get_icon_path("text-x-generic", theme_name)
        .or_else(|| get_icon_path("inode-file", theme_name))
}

pub fn get_error_icon_path(theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    get_icon_path("dialog-error", theme_name)
        .or_else(|| get_icon_path("system-file-locked", theme_name))
}

pub fn get_audio_icon_path(theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    get_icon_path("audio-x-generic", theme_name)
        .or_else(|| get_icon_path("x-content-audio", theme_name))
}

/// Icon names that pikeru actually uses, for filtering out empty themes.
const PIKERU_ICON_NAMES: &[&str] = &[
    "folder",
    "text-x-generic",
    "dialog-error",
    "audio-x-generic",
    "inode-file",
    "x-office-document",
    "x-content-audio",
];

/// Discover all installed icon themes that contain at least one icon pikeru actually needs.
///
/// Filters out themes that have no usable icons (e.g. `hicolor`, `HighContrast`, or
/// skeleton themes that exist only as containers).
pub fn discover_themes() -> Vec<String> {
    use linicon::themes;
    themes().into_iter().filter(|theme| {
            PIKERU_ICON_NAMES.iter().any(|name| { lookup_icon(name).from_theme(&theme.name).next().is_some() })
        }).map(|t| t.name).collect()
}
