//! XDG icon theme lookup using linicon.
//!
//! Resolves standard icon names (e.g. "folder", "audio-x-generic") to actual
//! image files on disk, falling back to bundled assets when no system icon is found.
//!
//! Font discovery: uses `fc-list` to find available system fonts and `ttf-parser`
//! to read their internal family names.
use linicon::{IconType, lookup_icon};
use std::collections::HashSet;
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
///
/// If `theme_name` is `Some("None")`, the system lookup is skipped entirely
/// and `None` is returned, forcing the caller to use bundled fallback icons.
pub fn get_icon_path(icon_name: &str, theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    // "None" means skip system icons entirely
    if theme_name.is_none() || theme_name == Some("None") { return None; }
    let theme_name = if theme_name.as_deref() == Some("System default") { None } else { theme_name };
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

/// Discover all installed icon themes that contain a folder icon.
///
/// Filters out themes that lack a folder icon (e.g. cursor-only themes like
/// `Banana`, or skeleton themes that exist only as containers).
pub fn discover_themes() -> Vec<String> {
    use linicon::themes;
    themes().into_iter().filter(|theme| {
            lookup_icon("folder")
                .from_theme(&theme.name)
                .use_fallback_themes(false)
                .next()
                .is_some()
        }).map(|t| t.name).collect()
}

/// Extracts the internal family name from a font file using ttf-parser.
///
/// This reads the font file's name table to get the real family name that
/// the text renderer will use for lookups.
///
/// Returns `None` if the font file can't be read or has no name table.
pub fn get_font_internal_name(path: &PathBuf) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let font = ttf_parser::Face::parse(&bytes, 0).ok()?;
    font.names()
        .into_iter()
        .find(|name| name.name_id == ttf_parser::name_id::FAMILY)
        .and_then(|name| name.to_string())
}

/// Checks if a font's charset string (from fc-list) covers basic Latin characters.
///
/// Verifies the font has support for ALL of: A-Z, a-z, 0-9
/// (Unicode codepoints 0x41-0x5A, 0x61-0x7A, 0x30-0x39).
/// Filters out emoji fonts, script-specific fonts, and other fonts that would show tofu.
fn has_latin_support(charset: &str) -> bool {
    // Check that ALL three required ranges are present
    let mut has_upper = false;
    let mut has_lower = false;
    let mut has_digits = false;

    for range in charset.split_whitespace() {
        let (start, end) = match range.split_once('-') {
            Some((s, e)) => {
                (u32::from_str_radix(s, 16).unwrap_or(0),
                 u32::from_str_radix(e, 16).unwrap_or(0))
            }
            None => {
                let codepoint = u32::from_str_radix(range, 16).unwrap_or(0);
                (codepoint, codepoint)
            }
        };

        // Check if this range fully covers each required block
        if start <= 0x41 && end >= 0x5A { has_upper = true; }
        if start <= 0x61 && end >= 0x7A { has_lower = true; }
        if start <= 0x30 && end >= 0x39 { has_digits = true; }
    }

    has_upper && has_lower && has_digits
}

/// Discover all available system fonts using `fc-list`.
///
/// Returns a deduplicated list of `(display_name, font_file_path)` pairs.
/// The display name is the fontconfig family name; the path is the actual
/// font file (`.ttf`, `.otf`, etc.).
///
/// Only includes fonts that have:
/// 1. A valid internal family name (can be loaded by iced)
/// 2. Basic Latin character support (filters out script-specific fonts that show tofu)
async fn discover_fonts() -> Vec<(String, PathBuf)> {
    let output = match std::process::Command::new("fc-list")
        .arg("--format=%{family}\t%{file}\t%{charset}\n")
        .output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let lines = match std::str::from_utf8(&output.stdout) {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };

    let mut seen = HashSet::new();
    let mut fonts = Vec::new();

    for line in lines.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let family = parts[0].to_string();
        // Skip if we've already seen this family
        if !seen.insert(family.clone()) {
            continue;
        }

        let path = PathBuf::from(parts[1]);

        // Check charset if available
        if parts.len() >= 3 {
            if !has_latin_support(parts[2]) {
                continue; // Skip fonts without Latin support
            }
        }

        // Only include fonts that have a valid internal name (can be loaded by iced)
        if get_font_internal_name(&path).is_some() {
            fonts.push((family, path));
        }
    }

    fonts
}

/// Async wrapper for theme and font discovery.
///
/// If either list is already populated, reuses the existing result
/// to avoid redundant work.
pub async fn discover_themes_async(
    existing_themes: Option<Vec<String>>,
    existing_fonts: Option<Vec<(String, PathBuf)>>,
) -> (Vec<String>, Vec<(String, PathBuf)>) {
    let themes = existing_themes.unwrap_or_else(|| discover_themes());
    let fonts = match existing_fonts {
        Some(fonts) => fonts,
        None => discover_fonts().await,
    };
    (themes, fonts)
}
