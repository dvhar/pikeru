//! XDG icon theme lookup using linicon.
//!
//! Resolves standard icon names (e.g. "folder", "audio-x-generic") to actual
//! image files on disk, falling back to bundled assets when no system icon is found.
use linicon::{IconType, lookup_icon};
use std::path::PathBuf;

/// Resolves an icon name to a local file path using the XDG Icon Theme spec.
///
/// Returns `Some(path, icon_type)` if a matching icon is found in any installed
/// icon theme, or `None` if no match exists.
pub fn get_icon_path(icon_name: &str) -> Option<(PathBuf, IconType)> {
    lookup_icon(icon_name).next().and_then(|result| {
        result.ok().map(|icon_path| (icon_path.path, icon_path.icon_type))
    })
}

/// Specific helpers for common icons used by pikeru.
pub fn get_folder_icon_path() -> Option<(PathBuf, IconType)> {
    get_icon_path("folder")
}

pub fn get_document_icon_path() -> Option<(PathBuf, IconType)> {
    get_icon_path("text-x-generic")
        .or_else(|| get_icon_path("x-office-document"))
        .or_else(|| get_icon_path("application-x-executable"))
}

pub fn get_unknown_icon_path() -> Option<(PathBuf, IconType)> {
    get_icon_path("text-x-generic")
        .or_else(|| get_icon_path("inode-file"))
}

pub fn get_error_icon_path() -> Option<(PathBuf, IconType)> {
    get_icon_path("dialog-error")
        .or_else(|| get_icon_path("system-file-locked"))
}

pub fn get_audio_icon_path() -> Option<(PathBuf, IconType)> {
    get_icon_path("audio-x-generic")
        .or_else(|| get_icon_path("x-content-audio"))
}

/// Human-readable label for an icon type.
pub fn icon_type_label(t: &IconType) -> &'static str {
    match t {
        IconType::PNG => "PNG",
        IconType::SVG => "SVG",
        IconType::XMP => "XPM",
    }
}
