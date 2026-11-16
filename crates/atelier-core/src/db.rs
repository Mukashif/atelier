//! SQLite storage (spec §3). One file, WAL mode, small row updates.
//!
//! Scanned facts and user edits live in the same `projects` row. A scan only
//! touches scanned columns; edits are never overwritten by a scan. Projects
//! that disappear from disk are kept with `present = 0` so their edits survive
//! a rename until the user re-links or forgets them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scan::{CoverKind, ScannedProject};

pub const DEFAULT_CATEGORIES: &[&str] = &["Book Covers", "Typography", "Misc"];
pub const DEFAULT_STATUSES: &[&str] = &["In-Progress", "Done", "Delivered"];
pub const UNCATEGORISED: &str = "Uncategorised";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("project {0} not found")]
    NotFound(i64),
}

pub type Result<T> = std::result::Result<T, DbError>;

pub struct Db {
    conn: Connection,
}

/// One project as the UI sees it, with defaults already applied.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: i64,
    pub rel_path: String,
    pub title: String,
    pub folder_name: String,
    pub month_folder: String,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub category: String,
    pub status: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub present: bool,
    /// "image", "unsupported" or null when the folder has no cover at all.
    pub cover_kind: Option<String>,
    pub cover_rel: Option<String>,
    pub manual_cover: Option<String>,
    pub cover_mtime: Option<i64>,
    pub last_opened_at: Option<i64>,
    pub favorited_at: Option<i64>,
    pub archived_at: Option<i64>,
}

impl ProjectRow {
    /// Absolute path of the image to thumbnail/show: the picked cover, else the
    /// detected one. None when there is nothing previewable.
    pub fn cover_source(&self, root: &Path) -> Option<PathBuf> {
        if let Some(m) = &self.manual_cover {
            return Some(PathBuf::from(m));
        }
        if self.cover_kind.as_deref() == Some(CoverKind::Image.as_str()) {
            let rel = self.cover_rel.as_ref()?;
            let mut p = root.join(rel_to_path(&self.rel_path));
            p.push(rel_to_path(rel));
            return Some(p);
        }
        None
    }

    pub fn folder_path(&self, root: &Path) -> PathBuf {
        root.join(rel_to_path(&self.rel_path))
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub restored: usize,
    /// Entries whose folder isn't in the library (kept as missing projects).
    pub missing: usize,
}

/// A project whose folder wasn't found, with a likely replacement.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Orphan {
    pub project: ProjectRow,
    pub suggestion: Option<ProjectRow>,
}

/// `a/b/c` → platform path.
pub fn rel_to_path(rel: &str) -> PathBuf {
    rel.split('/').collect()
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDiff {
    pub added: usize,
    pub missing: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Name,
}

/// What the Library/Favorites/Archive views ask for.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    /// false → hide archived; true → only archived.
    pub archived: bool,
    pub favorites_only: bool,
    pub search: Option<String>,
    pub categories: Vec<String>,
    pub statuses: Vec<String>,
    pub tags: Vec<String>,
    /// Include projects whose folder was not found on the last scan.
    pub include_missing: bool,
    pub sort: Sort,
}

/// One user edit. Applied to one or many projects.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Edit {
    Title(String),
    Status(String),
    Category(String),
    Favorite(bool),
    Archived(bool),
    AddTag(String),
    RemoveTag(String),
    ManualCover(Option<String>),
    Opened(i64),
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
  id             INTEGER PRIMARY KEY,
  rel_path       TEXT NOT NULL UNIQUE,
  folder_name    TEXT NOT NULL,
  month_folder   TEXT NOT NULL,
  year           INTEGER,
  month          INTEGER,
  cover_rel      TEXT,
  cover_mtime    INTEGER,
  cover_kind     TEXT,
  present        INTEGER NOT NULL DEFAULT 1,
  title_override TEXT,
  category       TEXT,
  status         TEXT,
  manual_cover   TEXT,
  is_favorite    INTEGER NOT NULL DEFAULT 0,
  favorited_at   INTEGER,
  is_archived    INTEGER NOT NULL DEFAULT 0,
  archived_at    INTEGER,
  last_opened_at INTEGER
);
CREATE INDEX IF NOT EXISTS projects_date ON projects(year, month);
CREATE TABLE IF NOT EXISTS project_tags (
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  tag        TEXT NOT NULL,
  PRIMARY KEY (project_id, tag)
);
"#;

const ROW_COLUMNS: &str = "id, rel_path, folder_name, month_folder, year, month, cover_rel, cover_mtime, \
     cover_kind, present, title_override, category, status, manual_cover, is_favorite, favorited_at, \
     is_archived, archived_at, last_opened_at";

struct Defaults {
    category: String,
    status: String,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        Ok(Self { conn })
    }

    // ---------------------------------------------------------------- settings

    pub fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        Ok(match raw {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        })
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn root_folder(&self) -> Result<Option<PathBuf>> {
        Ok(self
            .get_setting("rootFolder")?
            .and_then(|v| v.as_str().map(PathBuf::from)))
    }

    pub fn set_root_folder(&self, root: &Path) -> Result<()> {
        self.set_setting("rootFolder", &Value::String(root.to_string_lossy().into_owned()))
    }

    fn string_list(&self, key: &str, default: &[&str]) -> Result<Vec<String>> {
        Ok(match self.get_setting(key)? {
            Some(Value::Array(items)) => items
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => default.iter().map(|s| s.to_string()).collect(),
        })
    }

    pub fn categories(&self) -> Result<Vec<String>> {
        self.string_list("categories", DEFAULT_CATEGORIES)
    }

    pub fn statuses(&self) -> Result<Vec<String>> {
        self.string_list("statuses", DEFAULT_STATUSES)
    }

    fn defaults(&self) -> Result<Defaults> {
        let category = match self.get_setting("defaultCategory")? {
            Some(Value::String(s)) => s,
            _ => self.categories()?.into_iter().next().unwrap_or_else(|| UNCATEGORISED.into()),
        };
        let status = self.statuses()?.into_iter().next().unwrap_or_else(|| "In-Progress".into());
        Ok(Defaults { category, status })
    }

    // -------------------------------------------------------------------- scan

    /// Merge a scan into the store. Scanned columns are refreshed; edit columns
    /// are never touched. Projects not seen are marked missing, not deleted.
    pub fn apply_scan(&mut self, projects: &[ScannedProject]) -> Result<ScanDiff> {
        let tx = self.conn.transaction()?;
        let existing: HashSet<String> = {
            let mut stmt = tx.prepare("SELECT rel_path FROM projects")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        let mut diff = ScanDiff { total: projects.len(), ..Default::default() };
        {
            let mut upsert = tx.prepare(
                "INSERT INTO projects (rel_path, folder_name, month_folder, year, month,
                                       cover_rel, cover_mtime, cover_kind, present)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)
                 ON CONFLICT(rel_path) DO UPDATE SET
                   folder_name = excluded.folder_name, month_folder = excluded.month_folder,
                   year = excluded.year, month = excluded.month, cover_rel = excluded.cover_rel,
                   cover_mtime = excluded.cover_mtime, cover_kind = excluded.cover_kind, present = 1",
            )?;
            for p in projects {
                if !existing.contains(&p.rel_path) {
                    diff.added += 1;
                }
                upsert.execute(params![
                    p.rel_path,
                    p.folder_name,
                    p.month_folder,
                    p.year,
                    p.month,
                    p.cover_rel,
                    p.cover_mtime,
                    p.cover_kind.map(CoverKind::as_str),
                ])?;
            }
        }
        let seen: HashSet<&str> = projects.iter().map(|p| p.rel_path.as_str()).collect();
        {
            let mut mark = tx.prepare("UPDATE projects SET present = 0 WHERE rel_path = ?1 AND present = 1")?;
            for rel in existing.iter().filter(|r| !seen.contains(r.as_str())) {
                diff.missing += mark.execute([rel])?;
            }
        }
        tx.commit()?;
        Ok(diff)
    }

    // ------------------------------------------------------------------- reads

    pub fn list(&self, q: &Query) -> Result<Vec<ProjectRow>> {
        let d = self.defaults()?;
        let mut sql = format!("SELECT {ROW_COLUMNS} FROM projects WHERE is_archived = ?");
        let mut args: Vec<rusqlite::types::Value> = vec![(q.archived as i64).into()];
        if !q.include_missing {
            sql.push_str(" AND present = 1");
        }
        if q.favorites_only {
            sql.push_str(" AND is_favorite = 1");
        }
        if !q.categories.is_empty() {
            sql.push_str(&format!(" AND COALESCE(category, ?) IN ({})", placeholders(q.categories.len())));
            args.push(d.category.clone().into());
            args.extend(q.categories.iter().cloned().map(Into::into));
        }
        if !q.statuses.is_empty() {
            sql.push_str(&format!(" AND COALESCE(status, ?) IN ({})", placeholders(q.statuses.len())));
            args.push(d.status.clone().into());
            args.extend(q.statuses.iter().cloned().map(Into::into));
        }
        if !q.tags.is_empty() {
            sql.push_str(&format!(
                " AND id IN (SELECT project_id FROM project_tags WHERE tag IN ({}))",
                placeholders(q.tags.len())
            ));
            args.extend(q.tags.iter().cloned().map(Into::into));
        }
        if let Some(s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let like = format!("%{}%", escape_like(&s.to_lowercase()));
            sql.push_str(
                " AND (lower(COALESCE(title_override, folder_name)) LIKE ?1x ESCAPE '\\'
                       OR lower(folder_name) LIKE ?1x ESCAPE '\\'
                       OR id IN (SELECT project_id FROM project_tags WHERE lower(tag) LIKE ?1x ESCAPE '\\'))",
            );
            // positional: replace the marker with three plain placeholders
            sql = sql.replace("?1x", "?");
            args.extend(std::iter::repeat(like).take(3).map(Into::into));
        }
        sql.push_str(match q.sort {
            Sort::Newest => " ORDER BY year IS NULL, year DESC, month DESC, lower(COALESCE(title_override, folder_name))",
            Sort::Oldest => " ORDER BY year IS NULL, year ASC, month ASC, lower(COALESCE(title_override, folder_name))",
            Sort::Name => " ORDER BY lower(COALESCE(title_override, folder_name))",
        });

        let tags = self.all_tags_by_project()?;
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| row_from(r, &d))?;
        let mut out = Vec::new();
        for row in rows {
            let mut row = row?;
            row.tags = tags.get(&row.id).cloned().unwrap_or_default();
            out.push(row);
        }
        Ok(out)
    }

    pub fn get(&self, id: i64) -> Result<ProjectRow> {
        let d = self.defaults()?;
        let mut row = self
            .conn
            .query_row(&format!("SELECT {ROW_COLUMNS} FROM projects WHERE id = ?1"), [id], |r| row_from(r, &d))
            .optional()?
            .ok_or(DbError::NotFound(id))?;
        row.tags = self.tags_for(id)?;
        Ok(row)
    }

    pub fn id_for_rel_path(&self, rel: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM projects WHERE rel_path = ?1", [rel], |r| r.get(0))
            .optional()?)
    }

    fn tags_for(&self, id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT tag FROM project_tags WHERE project_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn all_tags_by_project(&self) -> Result<HashMap<i64, Vec<String>>> {
        let mut stmt = self.conn.prepare("SELECT project_id, tag FROM project_tags ORDER BY tag")?;
        let mut map: HashMap<i64, Vec<String>> = HashMap::new();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, tag) = row?;
            map.entry(id).or_default().push(tag);
        }
        Ok(map)
    }

    /// Every tag in use with its project count, most used first.
    pub fn tag_counts(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.tag, COUNT(*) FROM project_tags t JOIN projects p ON p.id = t.project_id
             WHERE p.is_archived = 0 AND p.present = 1 GROUP BY t.tag ORDER BY COUNT(*) DESC, t.tag",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ------------------------------------------------------------------ writes

    pub fn apply_edit(&mut self, ids: &[i64], edit: &Edit) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for &id in ids {
            changed += match edit {
                Edit::Title(t) => {
                    let t = t.trim();
                    if t.is_empty() {
                        0
                    } else {
                        tx.execute(
                            "UPDATE projects SET title_override = CASE WHEN ?2 = folder_name THEN NULL ELSE ?2 END
                             WHERE id = ?1",
                            params![id, t],
                        )?
                    }
                }
                Edit::Status(s) => tx.execute("UPDATE projects SET status = ?2 WHERE id = ?1", params![id, s])?,
                Edit::Category(c) => tx.execute("UPDATE projects SET category = ?2 WHERE id = ?1", params![id, c])?,
                Edit::Favorite(on) => tx.execute(
                    "UPDATE projects SET is_favorite = ?2,
                       favorited_at = CASE WHEN ?2 = 1 THEN COALESCE(favorited_at, ?3) ELSE NULL END
                     WHERE id = ?1",
                    params![id, *on as i64, now_ms()],
                )?,
                Edit::Archived(on) => tx.execute(
                    "UPDATE projects SET is_archived = ?2,
                       archived_at = CASE WHEN ?2 = 1 THEN ?3 ELSE NULL END
                     WHERE id = ?1",
                    params![id, *on as i64, now_ms()],
                )?,
                Edit::AddTag(t) => {
                    let t = t.trim();
                    if t.is_empty() {
                        0
                    } else {
                        tx.execute(
                            "INSERT OR IGNORE INTO project_tags(project_id, tag) VALUES (?1, ?2)",
                            params![id, t],
                        )?
                    }
                }
                Edit::RemoveTag(t) => {
                    tx.execute("DELETE FROM project_tags WHERE project_id = ?1 AND tag = ?2", params![id, t])?
                }
                Edit::ManualCover(p) => {
                    tx.execute("UPDATE projects SET manual_cover = ?2 WHERE id = ?1", params![id, p])?
                }
                Edit::Opened(at) => {
                    tx.execute("UPDATE projects SET last_opened_at = ?2 WHERE id = ?1", params![id, at])?
                }
            };
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Rename a category in the settings list and on every project using it.
    pub fn rename_category(&mut self, old: &str, new: &str) -> Result<usize> {
        self.rename_in_list("categories", DEFAULT_CATEGORIES, "category", old, new)
    }

    pub fn rename_status(&mut self, old: &str, new: &str) -> Result<usize> {
        self.rename_in_list("statuses", DEFAULT_STATUSES, "status", old, new)
    }

    fn rename_in_list(&mut self, key: &str, default: &[&str], column: &str, old: &str, new: &str) -> Result<usize> {
        let list: Vec<Value> = self
            .string_list(key, default)?
            .into_iter()
            .map(|s| Value::String(if s == old { new.to_string() } else { s }))
            .collect();
        self.set_setting(key, &Value::Array(list))?;
        let n = self
            .conn
            .execute(&format!("UPDATE projects SET {column} = ?2 WHERE {column} = ?1"), params![old, new])?;
        Ok(n)
    }

    pub fn set_categories(&self, list: &[String]) -> Result<()> {
        self.set_setting("categories", &serde_json::json!(list))
    }

    pub fn set_statuses(&self, list: &[String]) -> Result<()> {
        self.set_setting("statuses", &serde_json::json!(list))
    }

    /// Category given to projects nobody has categorised.
    pub fn default_category(&self) -> Result<String> {
        Ok(self.defaults()?.category)
    }

    // ----------------------------------------------------------------- orphans

    /// Projects whose folder wasn't found on the last scan, each with a likely
    /// match: a present project with the same folder name that has no edits.
    pub fn orphans(&self) -> Result<Vec<Orphan>> {
        let all = self.list(&Query { include_missing: true, ..Default::default() })?;
        let archived = self.list(&Query { include_missing: true, archived: true, ..Default::default() })?;
        let rows: Vec<ProjectRow> = all.into_iter().chain(archived).collect();
        let present: Vec<&ProjectRow> = rows.iter().filter(|r| r.present).collect();
        let mut out = Vec::new();
        for r in rows.iter().filter(|r| !r.present) {
            let name = r.folder_name.to_lowercase();
            let same_name: Vec<&&ProjectRow> = present.iter().filter(|p| p.folder_name.to_lowercase() == name).collect();
            let suggestion = same_name
                .iter()
                .find(|p| p.month_folder == r.month_folder)
                .or_else(|| same_name.first())
                .map(|p| (**p).clone());
            out.push(Orphan { project: r.clone(), suggestion });
        }
        Ok(out)
    }

    /// Move an orphan's edits onto a present project, then drop the orphan.
    pub fn relink(&mut self, orphan_id: i64, target_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let n = tx.execute(
            "UPDATE projects SET
               title_override = o.title_override, category = o.category, status = o.status,
               manual_cover = o.manual_cover, is_favorite = o.is_favorite, favorited_at = o.favorited_at,
               is_archived = o.is_archived, archived_at = o.archived_at,
               last_opened_at = COALESCE(o.last_opened_at, projects.last_opened_at)
             FROM (SELECT * FROM projects WHERE id = ?1) AS o
             WHERE projects.id = ?2",
            params![orphan_id, target_id],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(target_id));
        }
        tx.execute(
            "INSERT OR IGNORE INTO project_tags(project_id, tag) SELECT ?2, tag FROM project_tags WHERE project_id = ?1",
            params![orphan_id, target_id],
        )?;
        tx.execute("DELETE FROM projects WHERE id = ?1 AND present = 0", [orphan_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Forget a missing project and its edits.
    pub fn forget(&mut self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM projects WHERE id = ?1 AND present = 0", [id])?;
        Ok(())
    }

    // ------------------------------------------------------------------ backup

    /// `.atelier` backup (SPEC §7): settings + every project's edits, keyed by
    /// relative path. Scanned facts are left out; a rescan rebuilds them.
    pub fn export_backup(&self) -> Result<Value> {
        let mut settings = serde_json::Map::new();
        {
            let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (k, v) = row?;
                settings.insert(k, serde_json::from_str(&v)?);
            }
        }
        let tags = self.all_tags_by_project()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, rel_path, title_override, category, status, manual_cover, is_favorite, favorited_at,
                    is_archived, archived_at, last_opened_at FROM projects
             WHERE title_override IS NOT NULL OR category IS NOT NULL OR status IS NOT NULL
                OR manual_cover IS NOT NULL OR is_favorite = 1 OR is_archived = 1 OR last_opened_at IS NOT NULL
                OR id IN (SELECT project_id FROM project_tags)
             ORDER BY rel_path",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: i64 = r.get(0)?;
            Ok(serde_json::json!({
                "relPath": r.get::<_, String>(1)?,
                "titleOverride": r.get::<_, Option<String>>(2)?,
                "category": r.get::<_, Option<String>>(3)?,
                "status": r.get::<_, Option<String>>(4)?,
                "manualCover": r.get::<_, Option<String>>(5)?,
                "isFavorite": r.get::<_, i64>(6)? != 0,
                "favoritedAt": r.get::<_, Option<i64>>(7)?,
                "isArchived": r.get::<_, i64>(8)? != 0,
                "archivedAt": r.get::<_, Option<i64>>(9)?,
                "lastOpenedAt": r.get::<_, Option<i64>>(10)?,
                "tags": tags.get(&id).cloned().unwrap_or_default(),
            }))
        })?;
        let projects: Vec<Value> = rows.collect::<std::result::Result<_, _>>()?;
        Ok(serde_json::json!({
            "version": 1,
            "app": "atelier",
            "exportedAt": now_ms(),
            "settings": settings,
            "projects": projects,
        }))
    }

    /// Clear every edit (titles, statuses, categories, tags, favourites,
    /// archive, picked covers, last opened) and forget missing projects.
    /// Settings — including the category and status lists — are kept.
    pub fn reset_edits(&mut self) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let n = tx.execute(
            "UPDATE projects SET title_override = NULL, category = NULL, status = NULL, manual_cover = NULL,
               is_favorite = 0, favorited_at = NULL, is_archived = 0, archived_at = NULL, last_opened_at = NULL",
            [],
        )?;
        tx.execute("DELETE FROM project_tags", [])?;
        tx.execute("DELETE FROM projects WHERE present = 0", [])?;
        tx.commit()?;
        Ok(n)
    }

    /// Restore an `.atelier` backup: replaces settings (except the library
    /// folder) and every project's edits. Entries whose folder isn't known yet
    /// are kept as missing projects; a rescan links them up.
    pub fn restore_backup(&mut self, backup: &Value) -> Result<RestoreReport> {
        let bad = |m: &str| DbError::Json(serde::de::Error::custom(m.to_string()));
        if backup.get("version").and_then(Value::as_i64) != Some(1) || backup.get("app").and_then(Value::as_str) != Some("atelier") {
            return Err(bad("not an Atelier version-1 backup"));
        }
        let projects = backup.get("projects").and_then(Value::as_array).ok_or_else(|| bad("projects is missing"))?;
        let root = self.get_setting("rootFolder")?;

        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM settings", [])?;
        if let Some(settings) = backup.get("settings").and_then(Value::as_object) {
            for (k, v) in settings {
                if k == "rootFolder" {
                    continue;
                }
                tx.execute("INSERT INTO settings(key, value) VALUES (?1, ?2)", params![k, serde_json::to_string(v)?])?;
            }
        }
        if let Some(r) = root {
            tx.execute("INSERT INTO settings(key, value) VALUES ('rootFolder', ?1)", [serde_json::to_string(&r)?])?;
        }
        tx.execute(
            "UPDATE projects SET title_override = NULL, category = NULL, status = NULL, manual_cover = NULL,
               is_favorite = 0, favorited_at = NULL, is_archived = 0, archived_at = NULL, last_opened_at = NULL",
            [],
        )?;
        tx.execute("DELETE FROM project_tags", [])?;
        tx.execute("DELETE FROM projects WHERE present = 0", [])?;

        let mut report = RestoreReport::default();
        for p in projects {
            let Some(rel) = p.get("relPath").and_then(Value::as_str) else { continue };
            let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
            let i = |k: &str| p.get(k).and_then(Value::as_i64);
            let b = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
            let exists = tx
                .query_row("SELECT id FROM projects WHERE rel_path = ?1", [rel], |r| r.get::<_, i64>(0))
                .optional()?;
            let id = match exists {
                Some(id) => id,
                None => {
                    let parts: Vec<&str> = rel.split('/').collect();
                    let month_folder = parts.first().copied().unwrap_or_default();
                    let ym = crate::month::parse_month_folder(month_folder);
                    tx.execute(
                        "INSERT INTO projects (rel_path, folder_name, month_folder, year, month, present) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                        params![rel, parts.last().copied().unwrap_or_default(), month_folder, ym.map(|x| x.0), ym.map(|x| x.1)],
                    )?;
                    report.missing += 1;
                    tx.last_insert_rowid()
                }
            };
            tx.execute(
                "UPDATE projects SET title_override = ?2, category = ?3, status = ?4, manual_cover = ?5,
                   is_favorite = ?6, favorited_at = ?7, is_archived = ?8, archived_at = ?9, last_opened_at = ?10
                 WHERE id = ?1",
                params![id, s("titleOverride"), s("category"), s("status"), s("manualCover"), b("isFavorite") as i64,
                        i("favoritedAt"), b("isArchived") as i64, i("archivedAt"), i("lastOpenedAt")],
            )?;
            for t in p.get("tags").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                tx.execute("INSERT OR IGNORE INTO project_tags(project_id, tag) VALUES (?1, ?2)", params![id, t])?;
            }
            report.restored += 1;
        }
        tx.commit()?;
        Ok(report)
    }

    /// Low-level access for the importer (same crate only).
    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

fn row_from(r: &rusqlite::Row<'_>, d: &Defaults) -> rusqlite::Result<ProjectRow> {
    let folder_name: String = r.get(2)?;
    let title_override: Option<String> = r.get(10)?;
    Ok(ProjectRow {
        id: r.get(0)?,
        rel_path: r.get(1)?,
        title: title_override.unwrap_or_else(|| folder_name.clone()),
        folder_name,
        month_folder: r.get(3)?,
        year: r.get(4)?,
        month: r.get(5)?,
        cover_rel: r.get(6)?,
        cover_mtime: r.get(7)?,
        cover_kind: r.get(8)?,
        present: r.get::<_, i64>(9)? != 0,
        category: r.get::<_, Option<String>>(11)?.unwrap_or_else(|| d.category.clone()),
        status: r.get::<_, Option<String>>(12)?.unwrap_or_else(|| d.status.clone()),
        manual_cover: r.get(13)?,
        is_favorite: r.get::<_, i64>(14)? != 0,
        favorited_at: r.get(15)?,
        is_archived: r.get::<_, i64>(16)? != 0,
        archived_at: r.get(17)?,
        last_opened_at: r.get(18)?,
        tags: Vec::new(),
    })
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(rel: &str, year: Option<i32>, month: Option<u32>) -> ScannedProject {
        let (m, f) = rel.split_once('/').unwrap();
        ScannedProject {
            rel_path: rel.into(),
            folder_name: f.into(),
            month_folder: m.into(),
            year,
            month,
            cover_rel: Some("cover.jpg".into()),
            cover_kind: Some(CoverKind::Image),
            cover_mtime: Some(1),
            has_bookrack_thumb: false,
        }
    }

    #[test]
    fn scan_merge_keeps_edits_and_marks_missing() {
        let mut db = Db::open_in_memory().unwrap();
        let first = vec![sp("November 2025/A", Some(2025), Some(11)), sp("October 2025/B", Some(2025), Some(10))];
        let d = db.apply_scan(&first).unwrap();
        assert_eq!((d.added, d.missing), (2, 0));

        let a = db.id_for_rel_path("November 2025/A").unwrap().unwrap();
        db.apply_edit(&[a], &Edit::Status("Done".into())).unwrap();
        db.apply_edit(&[a], &Edit::AddTag("fantasy".into())).unwrap();

        let d = db.apply_scan(&first[..1]).unwrap();
        assert_eq!((d.added, d.missing), (0, 1));
        let row = db.get(a).unwrap();
        assert_eq!(row.status, "Done");
        assert_eq!(row.tags, vec!["fantasy".to_string()]);
        assert_eq!(db.list(&Query::default()).unwrap().len(), 1);
    }

    #[test]
    fn defaults_filters_search_and_sort() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[
            sp("November 2025/Salt Orchard", Some(2025), Some(11)),
            sp("October 2025/Ember", Some(2025), Some(10)),
            sp("Misc/Loose", None, None),
        ])
        .unwrap();
        let rows = db.list(&Query::default()).unwrap();
        assert_eq!(rows.iter().map(|r| r.folder_name.as_str()).collect::<Vec<_>>(), ["Salt Orchard", "Ember", "Loose"]);
        assert_eq!(rows[0].category, "Book Covers");
        assert_eq!(rows[0].status, "In-Progress");

        let ember = rows[1].id;
        db.apply_edit(&[ember], &Edit::Title("Nights Beneath Ember".into())).unwrap();
        db.apply_edit(&[ember], &Edit::Category("Typography".into())).unwrap();
        let q = Query { search: Some("beneath".into()), ..Default::default() };
        assert_eq!(db.list(&q).unwrap()[0].id, ember);
        let q = Query { categories: vec!["Typography".into()], ..Default::default() };
        assert_eq!(db.list(&q).unwrap().len(), 1);
        let q = Query { categories: vec!["Book Covers".into()], ..Default::default() };
        assert_eq!(db.list(&q).unwrap().len(), 2, "unedited projects count as the default category");

        db.apply_edit(&[ember], &Edit::Archived(true)).unwrap();
        assert_eq!(db.list(&Query::default()).unwrap().len(), 2);
        assert_eq!(db.list(&Query { archived: true, ..Default::default() }).unwrap().len(), 1);
    }

    #[test]
    fn title_equal_to_folder_clears_override() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[sp("2025-01/Folder", Some(2025), Some(1))]).unwrap();
        let id = db.id_for_rel_path("2025-01/Folder").unwrap().unwrap();
        db.apply_edit(&[id], &Edit::Title("New".into())).unwrap();
        assert_eq!(db.get(id).unwrap().title, "New");
        db.apply_edit(&[id], &Edit::Title("Folder".into())).unwrap();
        db.apply_edit(&[id], &Edit::Title("   ".into())).unwrap();
        assert_eq!(db.get(id).unwrap().title, "Folder");
    }

    #[test]
    fn orphan_relink_moves_edits() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[sp("May 2022/Fox", Some(2022), Some(5))]).unwrap();
        let old = db.id_for_rel_path("May 2022/Fox").unwrap().unwrap();
        db.apply_edit(&[old], &Edit::Status("Done".into())).unwrap();
        db.apply_edit(&[old], &Edit::AddTag("animals".into())).unwrap();
        db.apply_edit(&[old], &Edit::Favorite(true)).unwrap();
        // The folder moved to another month.
        db.apply_scan(&[sp("June 2022/Fox", Some(2022), Some(6))]).unwrap();

        let orphans = db.orphans().unwrap();
        assert_eq!(orphans.len(), 1);
        let target = orphans[0].suggestion.as_ref().unwrap().id;
        db.relink(old, target).unwrap();

        let row = db.get(target).unwrap();
        assert_eq!((row.status.as_str(), row.is_favorite), ("Done", true));
        assert_eq!(row.tags, vec!["animals".to_string()]);
        assert!(db.orphans().unwrap().is_empty());
        assert!(matches!(db.get(old), Err(DbError::NotFound(_))));
    }

    #[test]
    fn backup_contains_only_edited_projects() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[sp("2025-01/A", Some(2025), Some(1)), sp("2025-01/B", Some(2025), Some(1))]).unwrap();
        let a = db.id_for_rel_path("2025-01/A").unwrap().unwrap();
        db.apply_edit(&[a], &Edit::AddTag("x".into())).unwrap();
        db.set_categories(&["One".into()]).unwrap();
        let b = db.export_backup().unwrap();
        assert_eq!(b["version"], 1);
        assert_eq!(b["projects"].as_array().unwrap().len(), 1);
        assert_eq!(b["projects"][0]["relPath"], "2025-01/A");
        assert_eq!(b["settings"]["categories"][0], "One");
    }

    #[test]
    fn backup_round_trip_and_reset() {
        let mut db = Db::open_in_memory().unwrap();
        db.set_root_folder(Path::new("E:/Lib")).unwrap();
        db.apply_scan(&[sp("2025-01/A", Some(2025), Some(1)), sp("2025-01/B", Some(2025), Some(1))]).unwrap();
        let a = db.id_for_rel_path("2025-01/A").unwrap().unwrap();
        db.apply_edit(&[a], &Edit::Status("Done".into())).unwrap();
        db.apply_edit(&[a], &Edit::AddTag("x".into())).unwrap();
        db.apply_edit(&[a], &Edit::Favorite(true)).unwrap();
        db.set_categories(&["One".into(), "Two".into()]).unwrap();
        let backup = db.export_backup().unwrap();

        assert_eq!(db.reset_edits().unwrap(), 2);
        let row = db.get(a).unwrap();
        assert!(!row.is_favorite && row.tags.is_empty());
        assert_eq!(db.categories().unwrap(), ["One", "Two"], "reset keeps the lists");

        let mut extra = backup.clone();
        extra["projects"].as_array_mut().unwrap().push(serde_json::json!({ "relPath": "2024-05/Gone", "isArchived": true }));
        let report = db.restore_backup(&extra).unwrap();
        assert_eq!((report.restored, report.missing), (2, 1));
        let row = db.get(a).unwrap();
        assert_eq!((row.status.as_str(), row.is_favorite, row.tags.clone()), ("Done", true, vec!["x".to_string()]));
        assert_eq!(db.root_folder().unwrap(), Some(PathBuf::from("E:/Lib")), "restore keeps this PC's library folder");
        assert!(db.restore_backup(&serde_json::json!({ "version": 2 })).is_err());
    }

    #[test]
    fn rename_category_updates_projects() {
        let mut db = Db::open_in_memory().unwrap();
        db.apply_scan(&[sp("2025-01/A", Some(2025), Some(1))]).unwrap();
        let id = db.id_for_rel_path("2025-01/A").unwrap().unwrap();
        db.apply_edit(&[id], &Edit::Category("Misc".into())).unwrap();
        assert_eq!(db.rename_category("Misc", "Other").unwrap(), 1);
        assert_eq!(db.get(id).unwrap().category, "Other");
        assert!(db.categories().unwrap().contains(&"Other".to_string()));
    }
}
