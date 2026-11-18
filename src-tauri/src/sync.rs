//! Income tracker sync. Sends two things to the income tracker web app:
//!
//! - **Activity**: project folders with files changed since the start of the
//!   month of the last sync (first sync: this month). They become Inbox cards.
//! - **Covers**: every project whose category contains "cover", with its
//!   thumbnail when the income tracker asks for it.
//!
//! Nothing is sent until a sync key is saved. Folders are only read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atelier_core::db::{now_ms, Db, ProjectRow, Query};
use atelier_core::{activity, scan, thumbs};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const DEFAULT_URL: &str = "https://income-tracker-lemon.vercel.app";
const THUMB_BATCH: usize = 10;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    pub url: String,
    pub has_key: bool,
    pub auto_sync: bool,
    pub last_sync_at: Option<i64>,
}

pub fn settings(db: &Db) -> Result<SyncSettings, String> {
    let get = |k: &str| db.get_setting(k).map_err(|e| e.to_string());
    Ok(SyncSettings {
        url: get("syncUrl")?.and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| DEFAULT_URL.into()),
        has_key: get("syncKey")?.and_then(|v| v.as_str().map(|s| !s.is_empty())).unwrap_or(false),
        auto_sync: get("syncAfterScan")?.and_then(|v| v.as_bool()).unwrap_or(true),
        last_sync_at: get("lastSyncAt")?.and_then(|v| v.as_i64()),
    })
}

/// Everything a sync needs, read from the database in one go.
pub struct Plan {
    root: PathBuf,
    rows: Vec<ProjectRow>,
    url: String,
    key: String,
    since: i64,
    started: i64,
}

/// None when no key is saved (sync is off).
pub fn plan(db: &Db) -> Result<Option<Plan>, String> {
    let key = db.get_setting("syncKey").map_err(|e| e.to_string())?.and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    if key.is_empty() {
        return Ok(None);
    }
    let s = settings(db)?;
    let root = db.root_folder().map_err(|e| e.to_string())?.ok_or("Choose a library folder first.")?;
    let mut rows = db.list(&Query::default()).map_err(|e| e.to_string())?;
    rows.extend(db.list(&Query { archived: true, ..Default::default() }).map_err(|e| e.to_string())?);
    rows.retain(|r| r.present);
    let started = now_ms();
    // From the start of the month of the last sync, minus 6 hours so a file
    // saved just after midnight on the 1st in Pakistan (UTC+5) isn't missed.
    let since = month_start_utc(s.last_sync_at.unwrap_or(started)) - 6 * 3_600_000;
    Ok(Some(Plan { root, rows, url: s.url.trim_end_matches('/').to_string(), key, since, started }))
}

#[derive(Debug, Deserialize)]
struct SyncReply {
    files: i64,
    covers: i64,
    #[serde(default)]
    need_thumbs: Vec<String>,
    inbox_open: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub projects: usize,
    pub files: i64,
    pub covers: i64,
    pub thumbs: usize,
    pub inbox_open: i64,
    pub finished_at: i64,
}

/// Runs a sync. Slow parts (folder walk, network) happen without the database lock.
pub fn run(plan: Plan, cache_dir: &Path) -> Result<Outcome, String> {
    let dirs: Vec<PathBuf> = plan.rows.iter().map(|r| r.folder_path(&plan.root)).collect();
    let changed = activity::changed_in_projects(&dirs, plan.since);
    let activity_json: Vec<Value> = changed
        .iter()
        .map(|(i, files)| {
            let r = &plan.rows[*i];
            json!({ "rel_path": r.rel_path, "title": r.title, "month_folder": r.month_folder, "category": r.category, "files": files })
        })
        .collect();

    let cover_rows: Vec<&ProjectRow> = plan.rows.iter().filter(|r| r.category.to_lowercase().contains("cover")).collect();
    let small_thumb = |r: &ProjectRow| thumbs::thumb_path(cache_dir, r.id, thumbs::Size::Small);
    let covers_json: Vec<Value> = cover_rows
        .iter()
        .map(|r| {
            json!({
                "rel_path": r.rel_path, "title": r.title, "month_folder": r.month_folder,
                "year": r.year, "month": r.month, "thumb_version": scan::mtime_ms(&small_thumb(r)),
            })
        })
        .collect();

    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(60)).build();
    let reply: SyncReply = post(&agent, &plan, "/api/atelier/sync", &json!({ "activity": activity_json, "covers": covers_json }))?;

    // Send the thumbnails the income tracker doesn't have yet, a few at a time.
    let by_path: HashMap<&str, &ProjectRow> = cover_rows.iter().map(|r| (r.rel_path.as_str(), *r)).collect();
    let mut sent = 0;
    let wanted: Vec<&ProjectRow> = reply.need_thumbs.iter().filter_map(|p| by_path.get(p.as_str()).copied()).collect();
    for batch in wanted.chunks(THUMB_BATCH) {
        let items: Vec<Value> = batch
            .iter()
            .filter_map(|r| {
                let path = small_thumb(r);
                let bytes = std::fs::read(&path).ok()?;
                let data = format!("data:image/webp;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
                Some(json!({ "rel_path": r.rel_path, "version": scan::mtime_ms(&path), "data": data }))
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let r: Value = post(&agent, &plan, "/api/atelier/thumbs", &json!({ "thumbs": items }))?;
        sent += r.get("saved").and_then(Value::as_u64).unwrap_or(0) as usize;
    }

    Ok(Outcome {
        projects: changed.len(),
        files: reply.files,
        covers: reply.covers,
        thumbs: sent,
        inbox_open: reply.inbox_open,
        finished_at: plan.started,
    })
}

fn post<T: serde::de::DeserializeOwned>(agent: &ureq::Agent, plan: &Plan, path: &str, body: &Value) -> Result<T, String> {
    let url = format!("{}{}", plan.url, path);
    match agent.post(&url).set("Authorization", &format!("Bearer {}", plan.key)).send_json(body) {
        Ok(resp) => resp.into_json::<T>().map_err(|_| "The income tracker sent back something unexpected. Is the address right?".to_string()),
        Err(ureq::Error::Status(code, resp)) => {
            let msg = resp.into_json::<Value>().ok().and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string));
            Err(match (code, msg) {
                (_, Some(m)) => m,
                (404, None) => "The income tracker address doesn't have a sync page. Check the address in Settings.".into(),
                (c, None) => format!("The income tracker answered with an error ({c})."),
            })
        }
        Err(ureq::Error::Transport(t)) => Err(format!("Couldn't reach the income tracker ({}). Check your internet connection and the address.", t.kind())),
    }
}

/// Start of the month (UTC) that contains `ms`.
fn month_start_utc(ms: i64) -> i64 {
    let days = ms.div_euclid(86_400_000);
    let (y, m, _) = civil_from_days(days);
    days_from_civil(y, m, 1) * 86_400_000
}

// Howard Hinnant's date algorithms.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_start() {
        // 2026-11-18 12:00 UTC -> 2026-11-01 00:00 UTC
        assert_eq!(month_start_utc(1_795_003_200_000), 1_793_491_200_000);
        // 2024-03-01 00:00 UTC stays put; leap-year February before it.
        assert_eq!(month_start_utc(1_709_251_200_000), 1_709_251_200_000);
        assert_eq!(month_start_utc(1_709_251_199_999), 1_706_745_600_000);
    }
}

