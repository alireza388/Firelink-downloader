use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri_plugin_store::StoreExt;

const STORE_NAME: &str = "store.bin";
const OWNERSHIP_KEY: &str = "download_ownership";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadOwnershipRecord {
    id: String,
    primary_path: String,
}

pub fn expected_primary_path(
    app_handle: &tauri::AppHandle,
    destination: &str,
    filename: &str,
) -> Result<PathBuf, String> {
    let resolved_dest = crate::resolve_path(destination, app_handle);
    if !crate::is_safe_path(&resolved_dest, app_handle) {
        return Err("Path traversal blocked".to_string());
    }

    let safe_filename = Path::new(&filename.replace('\\', "/"))
        .file_name()
        .ok_or_else(|| "Download filename is invalid".to_string())?
        .to_owned();
    Ok(resolved_dest.join(safe_filename))
}

pub fn register_expected(
    app_handle: &tauri::AppHandle,
    id: &str,
    destination: &str,
    filename: &str,
) -> Result<(), String> {
    let path = expected_primary_path(app_handle, destination, filename)?;
    set_primary_path(app_handle, id, &path)
}

pub fn set_primary_path(
    app_handle: &tauri::AppHandle,
    id: &str,
    path: &Path,
) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Download ownership path must be absolute".to_string());
    }
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) {
        return Err("Download ownership path traversal is not allowed".to_string());
    }
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Download ownership path may not be a symlink".to_string());
    }

    let mut records = load_records(app_handle)?;
    records.retain(|record| record.id != id);
    records.push(DownloadOwnershipRecord {
        id: id.to_string(),
        primary_path: path.to_string_lossy().to_string(),
    });
    save_records(app_handle, records)
}

pub fn remove(app_handle: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let mut records = load_records(app_handle)?;
    let before = records.len();
    records.retain(|record| record.id != id);
    if records.len() == before {
        return Ok(());
    }
    save_records(app_handle, records)
}

pub fn known_primary_paths(app_handle: &tauri::AppHandle) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = load_records(app_handle)?
        .into_iter()
        .map(|record| PathBuf::from(record.primary_path))
        .collect();

    // One-time compatibility for downloads created before the backend-owned
    // registry existed. This imports the exact persisted queue path only.
    for path in legacy_download_queue_paths(app_handle)? {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }

    Ok(paths)
}

fn load_records(app_handle: &tauri::AppHandle) -> Result<Vec<DownloadOwnershipRecord>, String> {
    let store = app_handle
        .store(STORE_NAME)
        .map_err(|error| format!("Failed to load download ownership store: {error}"))?;
    store
        .get(OWNERSHIP_KEY)
        .map(|value| serde_json::from_value::<Vec<DownloadOwnershipRecord>>(value.clone()))
        .transpose()
        .map_err(|error| format!("Invalid download ownership data: {error}"))
        .map(|records| records.unwrap_or_default())
}

fn save_records(
    app_handle: &tauri::AppHandle,
    records: Vec<DownloadOwnershipRecord>,
) -> Result<(), String> {
    let store = app_handle
        .store(STORE_NAME)
        .map_err(|error| format!("Failed to load download ownership store: {error}"))?;
    let value = serde_json::to_value(records)
        .map_err(|error| format!("Failed to encode download ownership data: {error}"))?;
    store.set(OWNERSHIP_KEY, value);
    store
        .save()
        .map_err(|error| format!("Failed to save download ownership data: {error}"))
}

fn legacy_download_queue_paths(app_handle: &tauri::AppHandle) -> Result<Vec<PathBuf>, String> {
    let store = app_handle
        .store(STORE_NAME)
        .map_err(|error| format!("Failed to load download queue: {error}"))?;
    let settings = crate::settings::load_settings(app_handle).ok();
    let downloads = store
        .get("download_queue")
        .map(|value| serde_json::from_value::<Vec<crate::ipc::DownloadItem>>(value.clone()))
        .transpose()
        .map_err(|error| format!("Invalid download queue ownership data: {error}"))?
        .unwrap_or_default();

    let mut paths = Vec::new();
    for download in downloads {
        let category = format!("{:?}", download.category);
        let mut destinations = Vec::new();

        if let Some(destination) = download
            .destination
            .clone()
            .filter(|destination| !destination.trim().is_empty())
        {
            destinations.push(destination);
        }

        let category_destination = settings
            .as_ref()
            .and_then(|settings| settings.download_directories.get(&category).cloned());
        let default_destination = settings
            .as_ref()
            .map(|settings| settings.default_download_path.clone());

        if destinations.is_empty() {
            let fallback_destination = category_destination.clone().or(default_destination.clone());
            if let Some(destination) = fallback_destination {
                destinations.push(destination);
            }
        }

        if matches!(download.status, crate::ipc::DownloadStatus::Completed) {
            if let Some(destination) = category_destination {
                destinations.push(destination);
            }
            if let Some(destination) = default_destination {
                destinations.push(destination);
            }
            destinations.push("~/Downloads".to_string());
        }

        for destination in destinations {
            if let Ok(path) = expected_primary_path(app_handle, &destination, &download.file_name) {
                if !paths.iter().any(|existing| existing == &path) {
                    paths.push(path);
                }
            }
        }
    }

    Ok(paths)
}
