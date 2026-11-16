//! Importing from Bookrack (spec §7). Bookrack's files are only ever read.
//!
//! Sources: Bookrack's live `data/` folder (`settings.json` +
//! `projects-meta.json`) or a version-1 `.bookrack` backup. Absolute folder
//! paths are converted to paths relative to the library root; edits for
//! folders not found on disk are kept as missing ("orphan") projects.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::{Db, DbError};

/// What Bookrack displayed for projects without a category.
pub const BOOKRACK_DEFAULT_CATEGORY: &str = "Typography";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookrackMeta {
    pub folder_path: String,
    #[serde(default)]
    pub title_override: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub manual_thumbnail_path: Option<String>,
    #[serde(default)]
    pub is_archived: Option<bool>,
    #[serde(default)]
    pub is_favorite: Option<bool>,
    #[serde(default)]
    pub last_opened_at: Option<i64>,
    // Present in Bookrack but not carried over (kept in the raw copy).
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub sort_order: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct BookrackData {
    pub root_folder: Option<String>,
    pub categories: Option<Vec<String>>,
    pub statuses: Option<Vec<String>>,
    pub meta: Vec<BookrackMeta>,
    /// Raw files, saved untouched into Atelier's data folder.
    pub raw_files: Vec<(String, String)>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub orphans: usize,
    /// Entries whose folder isn't inside the library root (not imported).
    pub outside_root: Vec<String>,
    pub favorites: usize,
    pub archived: usize,
    pub tags: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("no Bookrack data found in {0}")]
    NotFound(PathBuf),
    #[error("could not read {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not valid Bookrack data: {1}")]
    Invalid(PathBuf, String),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Read Bookrack's live data folder (`…/bookrack/data`).
pub fn load_data_dir(dir: &Path) -> Result<BookrackData, ImportError> {
    let meta_path = dir.join("projects-meta.json");
    let settings_path = dir.join("settings.json");
    if !meta_path.is_file() {
        return Err(ImportError::NotFound(dir.to_path_buf()));
    }
    let meta_raw = read(&meta_path)?;
    let meta: Vec<BookrackMeta> =
        serde_json::from_str(&meta_raw).map_err(|e| ImportError::Invalid(meta_path.clone(), e.to_string()))?;
    let mut data = BookrackData { meta, ..Default::default() };
    data.raw_files.push(("projects-meta.json".into(), meta_raw));
    if settings_path.is_file() {
        let raw = read(&settings_path)?;
        let v: Value =
            serde_json::from_str(&raw).map_err(|e| ImportError::Invalid(settings_path.clone(), e.to_string()))?;
        apply_settings_value(&mut data, &v);
        data.raw_files.push(("settings.json".into(), raw));
    }
    Ok(data)
}

/// Read a `.bookrack` (or `.json`) backup. Valid only if `version == 1` and
/// `projectMeta` is an array.
pub fn load_backup(file: &Path) -> Result<BookrackData, ImportError> {
    let raw = read(file)?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| ImportError::Invalid(file.into(), e.to_string()))?;
    if v.get("version").and_then(Value::as_i64) != Some(1) {
        return Err(ImportError::Invalid(file.into(), "unsupported backup version".into()));
    }
    let meta = v
        .get("projectMeta")
        .filter(|m| m.is_array())
        .ok_or_else(|| ImportError::Invalid(file.into(), "projectMeta is missing".into()))?;
    let meta: Vec<BookrackMeta> =
        serde_json::from_value(meta.clone()).map_err(|e| ImportError::Invalid(file.into(), e.to_string()))?;
    let mut data = BookrackData { meta, ..Default::default() };
    if let Some(s) = v.get("settings") {
        apply_settings_value(&mut data, s);
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "backup.bookrack".into());
    data.raw_files.push((name, raw));
    Ok(data)
}

fn apply_settings_value(data: &mut BookrackData, v: &Value) {
    data.root_folder = v.get("rootFolder").and_then(Value::as_str).map(str::to_string);
    let list = |key: &str| -> Option<Vec<String>> {
        v.get(key)?.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
    };
    data.categories = list("categories");
    data.statuses = list("statuses");
}

fn read(p: &Path) -> Result<String, ImportError> {
    fs::read_to_string(p).map_err(|e| ImportError::Io(p.to_path_buf(), e))
}

/// `E:\Lib\Nov 2025\Proj` with root `E:\Lib` → `Nov 2025/Proj`.
/// Case-insensitive, either slash direction. None when outside the root.
pub fn to_rel_path(folder_path: &str, root: &str) -> Option<String> {
    let norm = |s: &str| {
        s.replace('\\', "/")
            .split('/')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let f = norm(folder_path);
    let r = norm(root);
    if f.len() <= r.len() {
        return None;
    }
    let inside = f.iter().zip(&r).all(|(a, b)| a.to_lowercase() == b.to_lowercase());
    inside.then(|| f[r.len()..].join("/"))
}

/// Write Bookrack's edits into Atelier. Run after a scan so existing projects
/// are matched; unmatched entries become missing projects with their edits.
/// Replaces any edits Atelier already holds for the same projects.
pub fn apply(db: &mut Db, data: &BookrackData, root: &Path) -> Result<ImportReport, ImportError> {
    if let Some(c) = &data.categories {
        db.set_setting("categories", &serde_json::json!(c))?;
    }
    if let Some(s) = &data.statuses {
        db.set_setting("statuses", &serde_json::json!(s))?;
    }
    // Bookrack showed "Typography" for projects never given a category (it was
    // hardcoded in its scanner). Keep showing what the user is used to.
    if db.categories()?.iter().any(|c| c == BOOKRACK_DEFAULT_CATEGORY) {
        db.set_setting("defaultCategory", &serde_json::json!(BOOKRACK_DEFAULT_CATEGORY))?;
    }
    let root_str = root.to_string_lossy().into_owned();
    let mut report = ImportReport::default();

    let tx = db.conn_mut().transaction().map_err(DbError::from)?;
    for m in &data.meta {
        let Some(rel) = to_rel_path(&m.folder_path, &root_str) else {
            report.outside_root.push(m.folder_path.clone());
            continue;
        };
        let parts: Vec<&str> = rel.split('/').collect();
        let folder_name = parts.last().copied().unwrap_or_default().to_string();
        let month_folder = parts.first().copied().unwrap_or_default().to_string();
        let ym = crate::month::parse_month_folder(&month_folder);

        let existed: bool = tx
            .query_row("SELECT 1 FROM projects WHERE rel_path = ?1", [&rel], |_| Ok(()))
            .map(|_| true)
            .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(false) } else { Err(e) })
            .map_err(DbError::from)?;
        if !existed {
            tx.execute(
                "INSERT INTO projects (rel_path, folder_name, month_folder, year, month, present)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                params![rel, folder_name, month_folder, ym.map(|x| x.0), ym.map(|x| x.1)],
            )
            .map_err(DbError::from)?;
            report.orphans += 1;
        }
        let title = m.title_override.as_deref().map(str::trim).filter(|t| !t.is_empty() && *t != folder_name);
        let fav = m.is_favorite.unwrap_or(false);
        let arch = m.is_archived.unwrap_or(false);
        tx.execute(
            "UPDATE projects SET title_override = ?2, category = ?3, status = ?4, manual_cover = ?5,
               is_favorite = ?6, favorited_at = CASE WHEN ?6 = 1 THEN COALESCE(favorited_at, ?9) ELSE NULL END,
               is_archived = ?7, archived_at = CASE WHEN ?7 = 1 THEN COALESCE(archived_at, ?9) ELSE NULL END,
               last_opened_at = ?8
             WHERE rel_path = ?1",
            params![
                rel,
                title,
                m.category,
                m.status,
                m.manual_thumbnail_path,
                fav as i64,
                arch as i64,
                m.last_opened_at,
                m.last_opened_at.unwrap_or_else(crate::db::now_ms),
            ],
        )
        .map_err(DbError::from)?;
        let id: i64 = tx
            .query_row("SELECT id FROM projects WHERE rel_path = ?1", [&rel], |r| r.get(0))
            .map_err(DbError::from)?;
        tx.execute("DELETE FROM project_tags WHERE project_id = ?1", [id]).map_err(DbError::from)?;
        for tag in m.tags.iter().flatten().map(|t| t.trim()).filter(|t| !t.is_empty()) {
            tx.execute("INSERT OR IGNORE INTO project_tags(project_id, tag) VALUES (?1, ?2)", params![id, tag])
                .map_err(DbError::from)?;
            report.tags += 1;
        }
        report.imported += 1;
        report.favorites += fav as usize;
        report.archived += arch as usize;
    }
    tx.commit().map_err(DbError::from)?;
    Ok(report)
}

/// Save the raw Bookrack files under `<data dir>/imports/bookrack-<ms>/`.
pub fn save_raw_copy(data: &BookrackData, app_data_dir: &Path) -> std::io::Result<PathBuf> {
    let dir = app_data_dir.join("imports").join(format!("bookrack-{}", crate::db::now_ms()));
    fs::create_dir_all(&dir)?;
    for (name, content) in &data.raw_files {
        fs::write(dir.join(name), content)?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Query;
    use crate::scan::{CoverKind, ScannedProject};

    #[test]
    fn rel_paths() {
        assert_eq!(to_rel_path(r"E:\Lib\Nov 2025\Proj", r"E:\Lib").as_deref(), Some("Nov 2025/Proj"));
        assert_eq!(to_rel_path(r"e:/lib/Nov 2025/Proj/", r"E:\Lib\").as_deref(), Some("Nov 2025/Proj"));
        assert_eq!(to_rel_path(r"D:\Other\Nov 2025\Proj", r"E:\Lib"), None);
        assert_eq!(to_rel_path(r"E:\Lib", r"E:\Lib"), None);
    }

    #[test]
    fn imports_edits_and_orphans() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[ScannedProject {
            rel_path: "November 2025/Salt".into(),
            folder_name: "Salt".into(),
            month_folder: "November 2025".into(),
            year: Some(2025),
            month: Some(11),
            cover_rel: Some("c.jpg".into()),
            cover_kind: Some(CoverKind::Image),
            cover_mtime: Some(1),
            has_bookrack_thumb: true,
        }])
        .unwrap();
        let data = BookrackData {
            meta: vec![
                BookrackMeta {
                    folder_path: r"E:\Lib\November 2025\Salt".into(),
                    title_override: Some("The Salt Orchard".into()),
                    category: Some("Typography".into()),
                    status: Some("Done".into()),
                    tags: Some(vec!["literary".into(), " ".into()]),
                    is_favorite: Some(true),
                    ..Default::default()
                },
                BookrackMeta {
                    folder_path: r"E:\Lib\May 2022\Gone".into(),
                    is_archived: Some(true),
                    ..Default::default()
                },
                BookrackMeta { folder_path: r"C:\Elsewhere\X\Y".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let report = apply(&mut db, &data, Path::new(r"E:\Lib")).unwrap();
        assert_eq!((report.imported, report.orphans, report.outside_root.len()), (2, 1, 1));
        assert_eq!((report.favorites, report.archived, report.tags), (1, 1, 1));

        let rows = db.list(&Query::default()).unwrap();
        assert_eq!(rows.len(), 1);
        let salt = &rows[0];
        assert_eq!((salt.title.as_str(), salt.category.as_str(), salt.status.as_str()), ("The Salt Orchard", "Typography", "Done"));
        assert!(salt.is_favorite);
        assert_eq!(salt.tags, vec!["literary".to_string()]);

        let missing = db.list(&Query { archived: true, include_missing: true, ..Default::default() }).unwrap();
        assert_eq!(missing.len(), 1);
        assert!(!missing[0].present);
        assert_eq!(missing[0].year, Some(2022));
    }

    #[test]
    fn backup_requires_version_1() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("b.bookrack");
        fs::write(&bad, r#"{"version":2,"projectMeta":[]}"#).unwrap();
        assert!(load_backup(&bad).is_err());
        let good = dir.path().join("g.bookrack");
        fs::write(&good, r#"{"version":1,"exportedAt":"x","settings":{"rootFolder":"E:\\Lib","categories":[]},"projectMeta":[{"folderPath":"E:\\Lib\\A\\B"}]}"#).unwrap();
        let d = load_backup(&good).unwrap();
        assert_eq!(d.meta.len(), 1);
        assert_eq!(d.categories, Some(vec![]));
    }
}
