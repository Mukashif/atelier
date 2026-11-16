//! Finding an existing Bookrack install's data folder (spec §7, step 1).
//! Only reads; never writes to Bookrack's files.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub data_dir: String,
    pub root_folder: Option<String>,
    pub projects: usize,
    pub favorites: usize,
    pub archived: usize,
    pub tags: usize,
}

/// Look in Bookrack's install folder (from Windows' uninstall entry) and the
/// usual per-user locations; return the first with readable data.
pub fn find() -> Option<Found> {
    candidates().into_iter().find_map(|dir| {
        let data = atelier_core::import::load_data_dir(&dir).ok()?;
        let mut tags = std::collections::HashSet::new();
        for m in &data.meta {
            tags.extend(m.tags.iter().flatten().cloned());
        }
        Some(Found {
            data_dir: dir.to_string_lossy().into_owned(),
            root_folder: data.root_folder.clone(),
            projects: data.meta.len(),
            favorites: data.meta.iter().filter(|m| m.is_favorite == Some(true)).count(),
            archived: data.meta.iter().filter(|m| m.is_archived == Some(true)).count(),
            tags: tags.len(),
        })
    })
}

fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    out.extend(registry_install_dirs().into_iter().map(|d| d.join("data")));
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(appdata) = env("APPDATA") {
        out.push(appdata.join("com.mani.bookrack").join("data"));
        out.push(appdata.join("com.mani.bookrack"));
    }
    if let Some(local) = env("LOCALAPPDATA") {
        out.push(local.join("bookrack").join("data"));
        out.push(local.join("Programs").join("bookrack").join("data"));
    }
    out
}

#[cfg(windows)]
fn registry_install_dirs() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let mut dirs = Vec::new();
    for (hive, path) in [
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    ] {
        let Ok(root) = RegKey::predef(hive).open_subkey(path) else { continue };
        for name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey(&name) else { continue };
            let display: String = key.get_value("DisplayName").unwrap_or_default();
            if !display.to_lowercase().contains("bookrack") {
                continue;
            }
            let loc: String = key.get_value("InstallLocation").unwrap_or_default();
            let loc = loc.trim().trim_matches('"');
            if !loc.is_empty() {
                dirs.push(PathBuf::from(loc));
            }
        }
    }
    dirs
}
