//! Discovering projects on disk (spec §4).
//!
//! Layout: `<root>/<month folder>/<project folder>`. Month folders are read in
//! parallel; an unreadable month folder is skipped and reported, never fatal.
//! Nothing here writes to the library.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::Serialize;

use crate::month::parse_month_folder;

/// Formats the webview can show directly.
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];
/// Likely cover files Atelier can't preview; recorded so the UI can say so.
pub const UNSUPPORTED_EXTS: &[&str] = &["psd", "psb", "tif", "tiff", "ai", "pdf", "heic", "indd"];
/// Bookrack's own thumbnail; never a cover source.
pub const BOOKRACK_THUMB: &str = ".bookrack_thumb.webp";
/// How deep below the project folder to look for a cover.
pub const MAX_COVER_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverKind {
    Image,
    Unsupported,
}

impl CoverKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CoverKind::Image => "image",
            CoverKind::Unsupported => "unsupported",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "image" => Some(CoverKind::Image),
            "unsupported" => Some(CoverKind::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScannedProject {
    /// `<month folder>/<project folder>`, always with `/`.
    pub rel_path: String,
    pub folder_name: String,
    pub month_folder: String,
    pub year: Option<i32>,
    pub month: Option<u32>,
    /// Cover path relative to the project folder, with `/`.
    pub cover_rel: Option<String>,
    pub cover_kind: Option<CoverKind>,
    /// Cover file mtime in ms since the epoch.
    pub cover_mtime: Option<i64>,
    /// Bookrack left a `.bookrack_thumb.webp` at the top of the project folder.
    pub has_bookrack_thumb: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct ScanResult {
    pub projects: Vec<ScannedProject>,
    pub month_folders: usize,
    /// Month folders that could not be read, with the error.
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("library folder not found: {0}")]
    RootMissing(PathBuf),
    #[error("could not read library folder {0}: {1}")]
    RootUnreadable(PathBuf, std::io::Error),
}

pub fn scan_root(root: &Path) -> Result<ScanResult, ScanError> {
    if !root.is_dir() {
        return Err(ScanError::RootMissing(root.to_path_buf()));
    }
    let entries = fs::read_dir(root).map_err(|e| ScanError::RootUnreadable(root.to_path_buf(), e))?;
    let month_dirs: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();

    let per_month: Vec<Result<Vec<ScannedProject>, (String, String)>> = month_dirs
        .par_iter()
        .map(|(name, path)| scan_month(name, path).map_err(|e| (name.clone(), e.to_string())))
        .collect();

    let mut result = ScanResult { month_folders: month_dirs.len(), ..Default::default() };
    for r in per_month {
        match r {
            Ok(mut projects) => result.projects.append(&mut projects),
            Err(skip) => {
                log::warn!("skipping month folder {}: {}", skip.0, skip.1);
                result.skipped.push(skip);
            }
        }
    }
    Ok(result)
}

fn scan_month(month_name: &str, month_path: &Path) -> std::io::Result<Vec<ScannedProject>> {
    let (year, month) = match parse_month_folder(month_name) {
        Some((y, m)) => (Some(y), Some(m)),
        None => (None, None),
    };
    let mut out = Vec::new();
    for entry in fs::read_dir(month_path)?.filter_map(Result::ok) {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        let dir = entry.path();
        let cover = find_cover(&dir);
        let (cover_rel, cover_kind, cover_mtime) = match cover {
            Some((path, kind)) => {
                let rel = rel_slash(&dir, &path);
                (rel, Some(kind), mtime_ms(&path))
            }
            None => (None, None, None),
        };
        out.push(ScannedProject {
            rel_path: format!("{month_name}/{folder_name}"),
            folder_name,
            month_folder: month_name.to_string(),
            year,
            month,
            cover_rel,
            cover_kind,
            cover_mtime,
            has_bookrack_thumb: dir.join(BOOKRACK_THUMB).is_file(),
        });
    }
    Ok(out)
}

/// First previewable image (listing order, this level before subfolders, max
/// depth 3). If there is none, the first likely cover in an unsupported format.
pub fn find_cover(project_dir: &Path) -> Option<(PathBuf, CoverKind)> {
    find_by_ext(project_dir, 0, IMAGE_EXTS)
        .map(|p| (p, CoverKind::Image))
        .or_else(|| find_by_ext(project_dir, 0, UNSUPPORTED_EXTS).map(|p| (p, CoverKind::Unsupported)))
}

fn find_by_ext(dir: &Path, depth: usize, exts: &[&str]) -> Option<PathBuf> {
    let entries: Vec<fs::DirEntry> = fs::read_dir(dir).ok()?.filter_map(Result::ok).collect();
    let mut subdirs = Vec::new();
    for e in &entries {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_file() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.eq_ignore_ascii_case(BOOKRACK_THUMB) {
                continue;
            }
            if has_ext(&name, exts) {
                return Some(e.path());
            }
        } else if ft.is_dir() {
            subdirs.push(e.path());
        }
    }
    if depth >= MAX_COVER_DEPTH {
        return None;
    }
    subdirs.iter().find_map(|d| find_by_ext(d, depth + 1, exts))
}

pub fn has_ext(name: &str, exts: &[&str]) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => exts.iter().any(|x| ext.eq_ignore_ascii_case(x)),
        _ => false,
    }
}

pub fn mtime_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn rel_slash(base: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, write};

    fn touch(p: &Path) {
        create_dir_all(p.parent().unwrap()).unwrap();
        write(p, b"x").unwrap();
    }

    #[test]
    fn finds_projects_and_covers() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        touch(&r.join("November 2025/Salt Orchard/front.JPG"));
        touch(&r.join("November 2025/Salt Orchard/.bookrack_thumb.webp"));
        touch(&r.join("November 2025/Deep One/a/b/cover.png"));
        touch(&r.join("November 2025/Too Deep/a/b/c/d/cover.png"));
        touch(&r.join("November 2025/Only PSD/cover_final.psd"));
        touch(&r.join("Misc/Loose/notes.txt"));
        touch(&r.join("stray.txt"));

        let mut res = scan_root(r).unwrap();
        res.projects.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        assert_eq!(res.month_folders, 2);
        assert_eq!(res.projects.len(), 5);

        let get = |rel: &str| res.projects.iter().find(|p| p.rel_path == rel).unwrap();
        let salt = get("November 2025/Salt Orchard");
        assert_eq!(salt.cover_rel.as_deref(), Some("front.JPG"));
        assert_eq!(salt.cover_kind, Some(CoverKind::Image));
        assert!(salt.has_bookrack_thumb);
        assert_eq!((salt.year, salt.month), (Some(2025), Some(11)));

        assert_eq!(get("November 2025/Deep One").cover_rel.as_deref(), Some("a/b/cover.png"));
        assert_eq!(get("November 2025/Too Deep").cover_rel, None);
        assert_eq!(get("November 2025/Only PSD").cover_kind, Some(CoverKind::Unsupported));
        let loose = get("Misc/Loose");
        assert_eq!((loose.year, loose.cover_rel.clone()), (None, None));
    }

    #[test]
    fn bookrack_thumb_is_never_the_cover() {
        let root = tempfile::tempdir().unwrap();
        touch(&root.path().join("2025-01/P/.bookrack_thumb.webp"));
        let res = scan_root(root.path()).unwrap();
        assert_eq!(res.projects[0].cover_rel, None);
        assert!(res.projects[0].has_bookrack_thumb);
    }

    #[test]
    fn missing_root_is_an_error() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(scan_root(&root.path().join("nope")), Err(ScanError::RootMissing(_))));
    }
}
