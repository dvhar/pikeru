//! XDG icon theme lookup using linicon.
//!
//! Resolves standard icon names (e.g. "folder", "audio-x-generic") to actual
//! image files on disk, falling back to bundled assets when no system icon is found.
//!
//! Font discovery: uses `fc-list` to find available system fonts and `ttf-parser`
//! to read their internal family names.
//!
//! File-type icon mapping: maps MIME types (derived from file extensions via
//! mime_guess) to XDG icon names, enabling thousands of distinct file type icons
//! from any installed icon theme.
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

// ---------------------------------------------------------------------------
// MIME-type → XDG icon name mapping
// ---------------------------------------------------------------------------
// Maps a MIME type string to the canonical XDG icon name that most icon themes
// use. Returns None when the MIME type should be handled by existing generic
// buckets (document, audio, unknown) or thumbnail generators.

/// Determine which "generic bucket" an extension falls into for existing icon
/// fields. Used as a fallback when no specific themed icon is found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GenericBucket {
    Document,
    Audio,
}

pub fn bucket_for_ext(ext: &str) -> Option<GenericBucket> {
    match ext {
        "txt"|"doc"|"docx"|"xls"|"xlsx"|"odt"|"ods"|"odp"|"odg"|"pdf"|"rtf"|"ppt"|"pptx" => Some(GenericBucket::Document),
        "mp3"|"wav"|"ogg"|"flac"|"aac"|"wma"|"aiff"|"alac"|"opus"|"m4a"|"weba" => Some(GenericBucket::Audio),
        _ => None,
    }
}

/// Map a MIME type to a *list* of XDG icon name candidates to try.
///
/// Returns an iterator of icon names ordered from most-specific to most-generic.
/// This allows the caller to try each one until the theme has it, providing
/// graceful fallback when an icon theme lacks specific MIME icons.
pub fn mime_icon_candidates(mime: &str) -> Option<Vec<&'static str>> {
    // --- Office / documents (specific types) ---
    if mime.contains("vnd.ms-") || mime.contains("vnd.openxmlformats-officedocument")
        || mime.contains("msword") || mime.contains("vnd.oasis.opendocument.text") {
        return Some(vec![
            "application-vnd.oasis.opendocument.text",
            "text-x-generic", "x-office-document",
        ]);
    }
    if mime.contains("spreadsheet") || mime.contains("vnd.ms-excel") {
        return Some(vec![
            "application-vnd.oasis.opendocument.spreadsheet",
            "x-office-spreadsheet", "package-x-generic",
        ]);
    }
    if mime.contains("presentation") || mime.contains("vnd.ms-powerpoint") {
        return Some(vec![
            "application-vnd.oasis.opendocument.presentation",
            "x-office-presentation", "package-x-generic",
        ]);
    }

    // --- Archives ---
    if mime.contains("zip") || mime.contains("7z") || mime.contains("rar")
        || mime.contains("tar") || mime.contains("gzip") || mime.contains("bzip2")
        || mime.contains("compress") || mime.contains("xz") || mime.contains("lzma")
        || mime.contains("x-compressed") || mime.contains("android.package-archive") {
        return Some(vec![
            "package-x-generic", "application-x-compressed",
            "file-archiver", "folder-archives", "inode-directory",
        ]);
    }

    // --- Programming / source code (with theme fallback chain) ---
    if mime.starts_with("text/x-") && !mime.contains("shell") && !mime.contains("script") {
        return Some(vec![
            "text-x-script", "text-x-generic",
        ]);
    }
    if mime.contains("javascript") || mime.contains("ecmascript") {
        return Some(vec![
            "application-javascript", "text-x-js", "text-x-script",
        ]);
    }
    if mime.contains("css") {
        return Some(vec![
            "text-css", "text-x-script", "text-x-generic",
        ]);
    }
    if mime.contains("json") {
        return Some(vec![
            "application-json", "text-x-script",
        ]);
    }
    if mime.contains("xml") || mime.contains("html") || mime.contains("svg+xml") {
        return Some(vec![
            "text-xml", "text-x-generic", "x-office-document",
        ]);
    }

    // --- Shell / scripts ---
    if mime.contains("shell") || mime.contains("x-shellscript") {
        return Some(vec![
            "application-x-shellscript", "application-x-executable",
        ]);
    }
    if mime.contains("python") {
        return Some(vec![
            "text-x-python", "text-x-script", "text-x-generic",
        ]);
    }

    // --- Marked-up text ---
    if mime.contains("markdown") {
        return Some(vec![
            "text-x-markdown", "text-x-generic",
        ]);
    }
    if mime.contains("yaml") || mime.contains("yml") {
        return Some(vec![
            "text-yaml", "text-x-script", "text-x-generic",
        ]);
    }

    // --- Fonts ---
    if mime.contains("font") || mime.contains("application/font") || mime.contains("application/vnd.ms-opentype") {
        return Some(vec![
            "x-office-document", "text-x-generic",
        ]);
    }

    // --- Disk images / ISOs ---
    if mime.contains("iso") || mime.contains("x-iso9660") || mime.contains("diskimage") {
        return Some(vec![
            "drive-harddisk", "media-optical",
        ]);
    }

    // --- Executables / binaries ---
    if mime.contains("executable") || mime.contains("ms-dos") || mime.contains("ms-win")
        || mime.contains("application/x-elf") || mime.contains("application/x-mach-binary") {
        return Some(vec![
            "application-x-executable", "application-x-dosexec",
            "system-run", "system-software-install",
        ]);
    }

    // --- Desktop / config files ---
    if mime.contains("desktop") || mime.contains("x-desktop") {
        return Some(vec![
            "application-x-desktop", "x-office-document",
        ]);
    }

    // --- Logs ---
    if mime.contains("log") {
        return Some(vec![
            "text-x-log", "text-x-generic",
        ]);
    }

    // --- Audio (application level, e.g. application/ogg) ---
    if mime.starts_with("audio/") || mime.contains("ogg") {
        return Some(vec![
            "audio-x-generic", "x-content-audio",
        ]);
    }

    // --- Generic text files (text/plain, etc.) ---
    if mime.starts_with("text/") {
        return Some(vec![
            "text-x-generic", "text-plain",
        ]);
    }

    // --- Generic application/octet-stream (unknown binary) ---
    if mime == "application/octet-stream" || mime.contains("octet") {
        return Some(vec![
            "application-octet-stream", "application-x-executable",
        ]);
    }

    None
}

/// Get an XDG icon path for a specific file type based on MIME type.
///
/// Returns the best available icon from the selected theme (SVG > PNG > XPM).
/// Returns `None` if no themed icon is found, in which case the caller should
/// fall back to generic buckets (`doc`, `audio`, `unknown`).
/// Resolve a MIME type or extension to an XDG icon path from the selected theme.
///
/// This is a convenience wrapper that tries MIME-based mapping first,
/// then falls back to extension-specific icon names. Currently unused by
/// `FItem::load()` (which uses `Icons::lookup_themed_icon` instead), but
/// kept as a utility for potential future use.
#[allow(dead_code)]
pub fn get_file_icon_path(mime: &str, ext: &str, theme_name: Option<&str>) -> Option<(PathBuf, IconType)> {
    // Check MIME-based candidates (each with its own fallback chain)
    if let Some(candidates) = mime_icon_candidates(mime) {
        for icon_name in &candidates {
            if let Some(path) = get_icon_path(icon_name, theme_name) {
                return Some(path);
            }
        }
    }

    // Try extension-specific icon names as a second pass.
    let ext_icons: &[&str] = match ext {
        "rs" => &["text-x-rustsrc", "text-x-script"],
        "go" => &["text-x-go", "text-x-script"],
        "rb" => &["text-x-ruby", "text-x-script"],
        "java" => &["text-x-java", "text-x-script"],
        "php" => &["text-x-php", "text-x-script"],
        "c" | "h" | "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx" => {
            &["text-x-c", "text-x-h"]
        }
        "cs" => &["text-x-csharp", "text-x-script"],
        "swift" => &["text-x-swift", "text-x-script"],
        "kt" | "kts" => &["text-x-kotlin", "text-x-script"],
        "ini" | "conf" | "cfg" => &["application-x-desktop", "text-x-config"],
        "toml" => &["text-toml", "application-json"],
        "csv" => &["application-vnd.oasis.opendocument.spreadsheet", "x-office-spreadsheet"],
        "md" | "mkd" => &["text-x-markdown", "text-x-script"],
        "gz" => &["application-gzip", "package-x-generic"],
        "bz2" => &["application-x-bzip2", "package-x-generic"],
        "xz" | "lzma" => &["application/x-xz", "package-x-generic"],
        "tgz" | "tar.gz" => &["application-x-tar", "package-x-generic"],
        "3gp" => &["video-3gpp", "video-x-generic"],
        "mid" | "midi" => &["audio-midi", "audio-x-generic"],
        "exr" => &["image-exr", "image-x-generic"],
        "psd" => &["image-psd", "image-x-generic"],
        "ai" => &["application-postscript", "image-x-generic"],
        "torrent" => &["application-x-bittorrent", "package-x-generic"],
        "vmdk" | "vdi" | "qcow2" => &["drive-harddisk", "package-x-generic"],
        _ => &[],
    };

    for icon_name in ext_icons {
        if let Some(path) = get_icon_path(icon_name, theme_name) {
            return Some(path);
        }
    }

    None
}

/// Check whether a MIME type indicates that pikeru should generate a thumbnail
/// rather than look up an icon name.
pub fn mime_needs_thumbnail(mime: &str) -> bool {
    // Thumbnail types: images, video, PDF, EPUB — handled by prepare_cached_thumbnail()
    mime.starts_with("image/") || mime.starts_with("video/")
        || mime == "application/pdf" || mime == "application/epub"
}

/// Check whether a MIME type indicates an audio file that should use the audio icon.
pub fn mime_is_audio(mime: &str) -> bool {
    mime.starts_with("audio/")
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
/// font file (`.ttf`, `.otf`, etc).
/// Deduplication is by both family name and file path: an entry is skipped
/// if either its family name or its file path has been seen before.
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

    let mut seen_families = HashSet::new();
    let mut seen_files = HashSet::new();
    let mut fonts = Vec::new();

    for line in lines.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let family = parts[0].to_string();
        let path = PathBuf::from(parts[1]);

        // Skip if we've already seen this family name or font file
        if !seen_families.insert(family.clone())
            || !seen_files.insert(path.clone())
        {
            continue;
        }

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
