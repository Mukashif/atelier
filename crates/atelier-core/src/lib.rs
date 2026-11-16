//! Atelier core: everything that isn't UI.
//!
//! - [`scan`]: find projects and covers under the library root (read-only).
//! - [`db`]: SQLite store of scanned facts and user edits.
//! - [`import`]: bring edits across from Bookrack.
//! - [`thumbs`]: cover thumbnails in the app cache.
//! - [`month`]: month-folder name parsing.

pub mod db;
pub mod import;
pub mod month;
pub mod scan;
pub mod thumbs;

use std::path::Path;

pub use db::{Db, Edit, ProjectRow, Query, Sort};

/// Build thumbnail jobs for every present project with a previewable cover.
pub fn thumb_jobs(rows: &[ProjectRow], root: &Path) -> Vec<thumbs::ThumbJob> {
    rows.iter()
        .filter(|r| r.present)
        .filter_map(|r| {
            let source = r.cover_source(root)?;
            Some(thumbs::ThumbJob {
                project_id: r.id,
                project_dir: r.manual_cover.is_none().then(|| r.folder_path(root)),
                source,
                force: false,
            })
        })
        .collect()
}
