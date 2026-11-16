//! Tauri shell: app state, commands, the `thumb` image protocol and
//! background work. All logic lives in `atelier-core`; this file wires it up.

mod bookrack;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use atelier_core::db::{now_ms, Db, Edit, ProjectRow, Query};
use atelier_core::{import, scan, thumb_jobs, thumbs};
use serde::Serialize;
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

pub struct AppState {
    db: Mutex<Db>,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    busy: AtomicBool,
}

type Shared = Arc<AppState>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    root: Option<String>,
    root_exists: bool,
    bookrack: Option<bookrack::Found>,
    categories: Vec<String>,
    statuses: Vec<String>,
    tags: Vec<(String, i64)>,
    counts: Counts,
    busy: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    library: usize,
    favorites: usize,
    archive: usize,
    missing: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    done: usize,
    total: usize,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ------------------------------------------------------------------ commands

#[tauri::command]
fn app_info(state: State<'_, Shared>) -> Result<AppInfo, String> {
    let db = state.db.lock().map_err(err)?;
    let root = db.root_folder().map_err(err)?;
    let count = |q: Query| db.list(&q).map(|v| v.len()).map_err(err);
    let counts = Counts {
        library: count(Query::default())?,
        favorites: count(Query { favorites_only: true, ..Default::default() })?,
        archive: count(Query { archived: true, ..Default::default() })?,
        missing: db
            .list(&Query { include_missing: true, ..Default::default() })
            .map_err(err)?
            .iter()
            .filter(|r| !r.present)
            .count(),
    };
    Ok(AppInfo {
        root_exists: root.as_ref().is_some_and(|r| r.is_dir()),
        root: root.map(|r| r.to_string_lossy().into_owned()),
        bookrack: if db.root_folder().map_err(err)?.is_none() { bookrack::find() } else { None },
        categories: db.categories().map_err(err)?,
        statuses: db.statuses().map_err(err)?,
        tags: db.tag_counts().map_err(err)?,
        counts,
        busy: state.busy.load(Ordering::Relaxed),
    })
}

#[tauri::command]
fn list_projects(state: State<'_, Shared>, query: Query) -> Result<Vec<ProjectRow>, String> {
    state.db.lock().map_err(err)?.list(&query).map_err(err)
}

#[tauri::command]
fn apply_edit(state: State<'_, Shared>, ids: Vec<i64>, edit: Edit) -> Result<usize, String> {
    state.db.lock().map_err(err)?.apply_edit(&ids, &edit).map_err(err)
}

#[tauri::command]
fn open_folder(app: AppHandle, state: State<'_, Shared>, id: i64) -> Result<(), String> {
    let path = {
        let mut db = state.db.lock().map_err(err)?;
        let root = db.root_folder().map_err(err)?.ok_or("no library folder")?;
        let row = db.get(id).map_err(err)?;
        db.apply_edit(&[id], &Edit::Opened(now_ms())).map_err(err)?;
        row.folder_path(&root)
    };
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
fn set_root(app: AppHandle, state: State<'_, Shared>, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    state.db.lock().map_err(err)?.set_root_folder(&p).map_err(err)?;
    start_rescan(app, state.inner().clone(), None);
    Ok(())
}

#[tauri::command]
fn rescan(app: AppHandle, state: State<'_, Shared>) {
    start_rescan(app, state.inner().clone(), None);
}

/// First-run import from Bookrack's data folder: set the library folder from
/// Bookrack's settings (if Atelier has none), scan, import, then thumbnails.
#[tauri::command]
fn import_bookrack(app: AppHandle, state: State<'_, Shared>, data_dir: String, root: Option<String>) -> Result<(), String> {
    let data = import::load_data_dir(Path::new(&data_dir)).map_err(err)?;
    let root = root
        .or_else(|| data.root_folder.clone())
        .ok_or("Bookrack has no library folder; choose one")?;
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(format!("library folder not found: {}", root.display()));
    }
    state.db.lock().map_err(err)?.set_root_folder(&root).map_err(err)?;
    if let Err(e) = import::save_raw_copy(&data, &state.data_dir) {
        log::warn!("could not keep a copy of Bookrack's files: {e}");
    }
    start_rescan(app, state.inner().clone(), Some(data));
    Ok(())
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsDto {
    root: Option<String>,
    data_dir: String,
    version: String,
    categories: Vec<String>,
    statuses: Vec<String>,
    default_category: String,
    open_on_launch: String,
    group_by_month: bool,
    reduce_motion: bool,
    check_updates_on_launch: bool,
    last_export_at: Option<i64>,
    counts: HashMap<String, i64>,
}

const PREF_KEYS: &[&str] = &["defaultCategory", "openOnLaunch", "groupByMonth", "reduceMotion", "checkUpdatesOnLaunch"];

#[tauri::command]
fn get_settings(app: AppHandle, state: State<'_, Shared>) -> Result<SettingsDto, String> {
    let db = state.db.lock().map_err(err)?;
    let get_bool = |k: &str, d: bool| -> Result<bool, String> {
        Ok(db.get_setting(k).map_err(err)?.and_then(|v| v.as_bool()).unwrap_or(d))
    };
    // How many projects use each category / status (for the Settings lists).
    let mut counts = HashMap::new();
    for r in db.list(&Query::default()).map_err(err)? {
        *counts.entry(format!("category:{}", r.category)).or_insert(0) += 1;
        *counts.entry(format!("status:{}", r.status)).or_insert(0) += 1;
    }
    Ok(SettingsDto {
        root: db.root_folder().map_err(err)?.map(|r| r.to_string_lossy().into_owned()),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        version: app.package_info().version.to_string(),
        categories: db.categories().map_err(err)?,
        statuses: db.statuses().map_err(err)?,
        default_category: db.default_category().map_err(err)?,
        open_on_launch: db
            .get_setting("openOnLaunch")
            .map_err(err)?
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "desk".into()),
        group_by_month: get_bool("groupByMonth", true)?,
        reduce_motion: get_bool("reduceMotion", false)?,
        check_updates_on_launch: get_bool("checkUpdatesOnLaunch", true)?,
        last_export_at: db.get_setting("lastExportAt").map_err(err)?.and_then(|v| v.as_i64()),
        counts,
    })
}

#[tauri::command]
fn set_pref(state: State<'_, Shared>, key: String, value: serde_json::Value) -> Result<(), String> {
    if !PREF_KEYS.contains(&key.as_str()) {
        return Err(format!("unknown setting {key}"));
    }
    state.db.lock().map_err(err)?.set_setting(&key, &value).map_err(err)
}

/// Replace the category or status list (add, delete, reorder).
#[tauri::command]
fn set_list(state: State<'_, Shared>, kind: String, items: Vec<String>) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    let items: Vec<String> = items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
        .collect();
    let db = state.db.lock().map_err(err)?;
    match kind.as_str() {
        "categories" => db.set_categories(&items),
        "statuses" => db.set_statuses(&items),
        _ => return Err(format!("unknown list {kind}")),
    }
    .map_err(err)
}

/// Rename a category/status everywhere; returns how many projects changed.
#[tauri::command]
fn rename_list_item(state: State<'_, Shared>, kind: String, old: String, new: String) -> Result<usize, String> {
    let new = new.trim();
    if new.is_empty() {
        return Err("name can't be empty".into());
    }
    let mut db = state.db.lock().map_err(err)?;
    match kind.as_str() {
        "categories" => db.rename_category(&old, new),
        "statuses" => db.rename_status(&old, new),
        _ => return Err(format!("unknown list {kind}")),
    }
    .map_err(err)
}

// ------------------------------------------------------------ covers & folders

/// Use a picked image as the cover; regenerate its thumbnails right away.
#[tauri::command]
fn set_cover(app: AppHandle, state: State<'_, Shared>, id: i64, path: String) -> Result<(), String> {
    let source = PathBuf::from(&path);
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !source.is_file() || !scan::has_ext(&name, scan::IMAGE_EXTS) {
        return Err("choose a JPG, PNG or WebP image".into());
    }
    state.db.lock().map_err(err)?.apply_edit(&[id], &Edit::ManualCover(Some(path))).map_err(err)?;
    let job = thumbs::ThumbJob { project_id: id, source, project_dir: None, force: true };
    let cache = state.cache_dir.clone();
    std::thread::spawn(move || {
        let _ = thumbs::ensure(&job, &cache);
        let _ = thumbs::ensure_large(&job, &cache);
        let _ = app.emit("thumbs-ready", vec![id]);
    });
    Ok(())
}

/// Open (creating if needed) the folder for the current month, e.g. "November 2025".
/// The only thing Atelier ever creates inside the library, and only on request.
#[tauri::command]
fn open_month_folder(app: AppHandle, state: State<'_, Shared>) -> Result<String, String> {
    let root = state.db.lock().map_err(err)?.root_folder().map_err(err)?.ok_or("no library folder")?;
    let (y, m) = current_year_month();
    let name = atelier_core::month::month_folder_name(y, m);
    // Reuse an existing folder for this month even if it's named differently.
    let existing = std::fs::read_dir(&root)
        .map_err(err)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .find(|e| atelier_core::month::parse_month_folder(&e.file_name().to_string_lossy()) == Some((y, m)))
        .map(|e| e.path());
    let dir = match existing {
        Some(d) => d,
        None => {
            let d = root.join(&name);
            std::fs::create_dir(&d).map_err(err)?;
            d
        }
    };
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(err)?;
    Ok(dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name))
}

/// Open the library folder ("root") or Atelier's data folder ("data").
#[tauri::command]
fn open_special(app: AppHandle, state: State<'_, Shared>, which: String) -> Result<(), String> {
    let path = match which.as_str() {
        "root" => state.db.lock().map_err(err)?.root_folder().map_err(err)?.ok_or("no library folder")?,
        "data" => state.data_dir.clone(),
        _ => return Err(format!("unknown folder {which}")),
    };
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(err)
}

/// Year and month for "now", in UTC (close enough for naming a month folder).
fn current_year_month() -> (i32, u32) {
    let days = now_ms().div_euclid(86_400_000);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
    (year, month)
}

// ----------------------------------------------------------------- orphans

#[tauri::command]
fn orphans(state: State<'_, Shared>) -> Result<Vec<atelier_core::db::Orphan>, String> {
    state.db.lock().map_err(err)?.orphans().map_err(err)
}

#[tauri::command]
fn relink(app: AppHandle, state: State<'_, Shared>, orphan_id: i64, target_id: i64) -> Result<(), String> {
    state.db.lock().map_err(err)?.relink(orphan_id, target_id).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
fn forget(app: AppHandle, state: State<'_, Shared>, id: i64) -> Result<(), String> {
    state.db.lock().map_err(err)?.forget(id).map_err(err)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
fn export_backup(state: State<'_, Shared>, path: String) -> Result<(), String> {
    let db = state.db.lock().map_err(err)?;
    let backup = db.export_backup().map_err(err)?;
    std::fs::write(&path, serde_json::to_string_pretty(&backup).map_err(err)?).map_err(err)?;
    db.set_setting("lastExportAt", &serde_json::json!(now_ms())).map_err(err)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreResult {
    kind: &'static str,
    restored: usize,
    missing: usize,
}

/// Restore an `.atelier` backup, or import a Bookrack `.bookrack` backup.
/// Replaces current edits, then rescans.
#[tauri::command]
fn restore_backup(app: AppHandle, state: State<'_, Shared>, path: String) -> Result<RestoreResult, String> {
    let raw = std::fs::read_to_string(&path).map_err(err)?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "That file isn't a backup Atelier can read.".to_string())?;
    let result = if v.get("app").and_then(|a| a.as_str()) == Some("atelier") {
        let report = state.db.lock().map_err(err)?.restore_backup(&v).map_err(err)?;
        RestoreResult { kind: "atelier", restored: report.restored, missing: report.missing }
    } else {
        let data = import::load_backup(Path::new(&path)).map_err(err)?;
        let mut db = state.db.lock().map_err(err)?;
        let root = db.root_folder().map_err(err)?.ok_or("choose a library folder first")?;
        db.reset_edits().map_err(err)?;
        let report = import::apply(&mut db, &data, &root).map_err(err)?;
        RestoreResult { kind: "bookrack", restored: report.imported, missing: report.orphans }
    };
    start_rescan(app, state.inner().clone(), None);
    Ok(result)
}

/// Clear every edit. If `backup_to` is given, export a backup there first;
/// nothing is cleared if that export fails.
#[tauri::command]
fn reset_data(app: AppHandle, state: State<'_, Shared>, backup_to: Option<String>) -> Result<usize, String> {
    let mut db = state.db.lock().map_err(err)?;
    if let Some(path) = backup_to {
        let backup = db.export_backup().map_err(err)?;
        std::fs::write(&path, serde_json::to_string_pretty(&backup).map_err(err)?)
            .map_err(|e| format!("Backup failed, nothing was reset: {e}"))?;
        db.set_setting("lastExportAt", &serde_json::json!(now_ms())).map_err(err)?;
    }
    let n = db.reset_edits().map_err(err)?;
    drop(db);
    let _ = app.emit("library-changed", ());
    Ok(n)
}

// --------------------------------------------------------- background work

/// Scan → merge → (import) → notify UI → thumbnails → notify UI. One at a time.
fn start_rescan(app: AppHandle, state: Shared, bookrack: Option<import::BookrackData>) {
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let result = run_rescan(&app, &state, bookrack);
        state.busy.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            log::error!("rescan failed: {e}");
            let _ = app.emit("scan-error", e);
        }
        let _ = app.emit("busy", false);
    });
}

fn run_rescan(app: &AppHandle, state: &Shared, bookrack: Option<import::BookrackData>) -> Result<(), String> {
    let _ = app.emit("busy", true);
    let root = state.db.lock().map_err(err)?.root_folder().map_err(err)?.ok_or("no library folder")?;
    let _ = app.emit("progress", Progress { phase: "scan", done: 0, total: 0 });
    let scanned = scan::scan_root(&root).map_err(err)?;
    let rows = {
        let mut db = state.db.lock().map_err(err)?;
        let diff = db.apply_scan(&scanned.projects).map_err(err)?;
        if let Some(data) = &bookrack {
            let report = import::apply(&mut db, data, &root).map_err(err)?;
            let _ = app.emit("import-done", report);
        }
        let _ = app.emit("library-changed", diff);
        db.list(&Query { include_missing: false, ..Default::default() })
            .map_err(err)?
            .into_iter()
            .chain(db.list(&Query { archived: true, ..Default::default() }).map_err(err)?)
            .collect::<Vec<_>>()
    };

    let jobs = thumb_jobs(&rows, &root);
    let stale: Vec<_> = jobs
        .into_iter()
        .filter(|j| thumbs::is_stale(&j.source, &thumbs::thumb_path(&state.cache_dir, j.project_id, thumbs::Size::Small)))
        .collect();
    let total = stale.len();
    // Report in batches so the UI updates a chunk of covers at a time.
    for (i, chunk) in stale.chunks(64).enumerate() {
        let report = thumbs::ensure_all(chunk, &state.cache_dir);
        let mut ready = report.generated;
        ready.extend(report.copied);
        let _ = app.emit("thumbs-ready", ready);
        let _ = app.emit("progress", Progress { phase: "thumbs", done: (i * 64 + chunk.len()).min(total), total });
    }
    let _ = app.emit("progress", Progress { phase: "idle", done: total, total });
    Ok(())
}

// ------------------------------------------------------------ thumb protocol

/// `thumb://<id>` (small) or `thumb://<id>@2x` (large, made on demand).
/// Serves only files from the thumbnail cache.
fn serve_thumb(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let not_found = || Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap();
    let key = path.trim_start_matches('/');
    let key = key.split(['?', '#']).next().unwrap_or_default();
    let key = percent_decode(key);
    let (id_str, large) = match key.strip_suffix("@2x") {
        Some(id) => (id, true),
        None => (key.as_str(), false),
    };
    let Ok(id) = id_str.parse::<i64>() else { return not_found() };
    let state = app.state::<Shared>();
    let file = if large {
        let job = {
            let Ok(db) = state.db.lock() else { return not_found() };
            let (Ok(Some(root)), Ok(row)) = (db.root_folder(), db.get(id)) else { return not_found() };
            let Some(source) = row.cover_source(&root) else { return not_found() };
            thumbs::ThumbJob { project_id: id, source, project_dir: None, force: false }
        };
        match thumbs::ensure_large(&job, &state.cache_dir) {
            Ok(p) => p,
            Err(_) => return not_found(),
        }
    } else {
        thumbs::thumb_path(&state.cache_dir, id, thumbs::Size::Small)
    };
    match std::fs::read(&file) {
        Ok(bytes) => Response::builder()
            .header("Content-Type", "image/webp")
            .header("Cache-Control", "no-cache")
            .header("Access-Control-Allow-Origin", "*")
            .body(bytes)
            .unwrap(),
        Err(_) => not_found(),
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// --------------------------------------------------------------------- run

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(&cache_dir)?;
            let db = Db::open(&data_dir.join("atelier.db"))?;
            let has_root = db.root_folder()?.is_some_and(|r| r.is_dir());
            let state: Shared = Arc::new(AppState {
                db: Mutex::new(db),
                data_dir,
                cache_dir,
                busy: AtomicBool::new(false),
            });
            app.manage(state.clone());
            // Show cached data immediately; refresh from disk in the background.
            if has_root {
                start_rescan(app.handle().clone(), state, None);
            }
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || responder.respond(serve_thumb(&app, &path)));
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_projects,
            apply_edit,
            open_folder,
            set_root,
            rescan,
            import_bookrack,
            get_settings,
            set_pref,
            set_list,
            rename_list_item,
            set_cover,
            open_month_folder,
            open_special,
            orphans,
            relink,
            forget,
            export_backup,
            restore_backup,
            reset_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running Atelier");
}
