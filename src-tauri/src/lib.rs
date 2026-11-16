//! Tauri shell: app state, commands, the `thumb` image protocol and
//! background work. All logic lives in `atelier-core`; this file wires it up.

mod bookrack;

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
            import_bookrack
        ])
        .run(tauri::generate_context!())
        .expect("error while running Atelier");
}
