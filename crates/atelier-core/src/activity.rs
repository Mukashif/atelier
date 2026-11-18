//! Finding recent work inside project folders, for the income tracker sync.
//!
//! A file counts as "work" when it was modified or created at or after
//! `since`. Creation time matters too: copying or unzipping files keeps their
//! old modified date but gives them a fresh creation date. Read-only.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::Serialize;

use crate::scan::BOOKRACK_THUMB;

/// How deep below the project folder to look.
pub const MAX_DEPTH: usize = 8;
/// At most this many files per project are reported (newest first).
pub const MAX_FILES: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedFile {
    /// Path relative to the project folder, with `/`.
    pub path: String,
    /// Latest of modified / created, in ms since the epoch.
    pub mtime: i64,
}

/// Files and folders that are never your work: system junk, Bookrack's
/// thumbnail, and print templates (KDP `PAPERBACK_…`/`CASE_LAMINATE_…`,
/// anything with "template" in the name, and their readme files).
pub fn is_ignored(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with('.')
        || lower.starts_with("~$")
        || lower == "thumbs.db"
        || lower == "desktop.ini"
        || lower == BOOKRACK_THUMB
        || lower == "readme.txt"
        || lower.ends_with(".tmp")
        || lower.ends_with(".crdownload")
        || lower.ends_with(".part")
        || lower.contains("template")
        || lower.starts_with("paperback_")
        || lower.starts_with("case_laminate_")
        || lower.starts_with("hardcover_")
}

fn stamp(meta: &fs::Metadata) -> Option<i64> {
    let ms = |t: std::time::SystemTime| t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as i64);
    let modified = meta.modified().ok().and_then(ms);
    let created = meta.created().ok().and_then(ms);
    modified.max(created)
}

/// Files in `project_dir` changed at or after `since_ms`, newest first.
pub fn changed_files(project_dir: &Path, since_ms: i64) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    walk(project_dir, project_dir, 0, since_ms, &mut out);
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)));
    out.truncate(MAX_FILES);
    out
}

fn walk(base: &Path, dir: &Path, depth: usize, since_ms: i64, out: &mut Vec<ChangedFile>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if is_ignored(&name) {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                walk(base, &e.path(), depth + 1, since_ms, out);
            }
        } else if ft.is_file() {
            let Ok(meta) = e.metadata() else { continue };
            let Some(t) = stamp(&meta) else { continue };
            if t >= since_ms {
                let path = e.path();
                let rel = path
                    .strip_prefix(base)
                    .map(|r| r.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
                    .unwrap_or_else(|_| name.to_string());
                out.push(ChangedFile { path: rel, mtime: t });
            }
        }
    }
}

/// `changed_files` for many projects at once (in parallel). Returns only
/// projects that had changes, as `(index into dirs, files)`.
pub fn changed_in_projects(dirs: &[std::path::PathBuf], since_ms: i64) -> Vec<(usize, Vec<ChangedFile>)> {
    dirs.par_iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let files = changed_files(d, since_ms);
            (!files.is_empty()).then_some((i, files))
        })
        .collect()
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
    fn ignores_templates_and_junk() {
        for n in [
            "PAPERBACK_6.000x9.000_776_BW_CREAM_en_US.pdf",
            "CASE_LAMINATE_5.500x8.500_367_BW_WHITE_en_US.zip",
            "AUDIOBOOK TEMPLATE.png",
            "readme.txt",
            "Thumbs.db",
            "desktop.ini",
            ".bookrack_thumb.webp",
            "~$draft.psd",
        ] {
            assert!(is_ignored(n), "{n} should be ignored");
        }
        for n in ["Wolfe Asylum - Paperback.pdf", "Raise the Dead - Wrap Update 2026.pdf", "9781738873111-Jacket.psd", "Hard Cover 1.psd"] {
            assert!(!is_ignored(n), "{n} is real work");
        }
    }

    #[test]
    fn finds_recent_files_and_skips_template_folders() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path();
        touch(&p.join("Book - Wrap.pdf"));
        touch(&p.join("Up/Book - 776 pages KDP.pdf"));
        touch(&p.join("PAPERBACK_6.000x9.000_776_BW_CREAM_en_US/inside.pdf"));
        touch(&p.join("Thumbs.db"));
        let files = changed_files(p, 0);
        let names: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"Book - Wrap.pdf"));
        assert!(names.contains(&"Up/Book - 776 pages KDP.pdf"));

        // Nothing is newer than the far future.
        assert!(changed_files(p, i64::MAX).is_empty());
    }
}
