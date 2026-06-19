use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_store::StoreExt;

#[tauri::command]
pub async fn reveal_in_file_manager(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    let primary = authorize_download_path(&app_handle, &path, DownloadAsset::Primary)?;
    let path = existing_download_asset(&primary).ok_or_else(|| {
        format!(
            "Downloaded file or partial file is missing: {}",
            primary.display()
        )
    })?;

    app_handle
        .opener()
        .reveal_item_in_dir(path.to_string_lossy().as_ref())
        .map_err(|e| format!("Failed to reveal file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn open_downloaded_file(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<(), String> {
    let path = authorize_download_path(&app_handle, &path, DownloadAsset::Primary)?;
    if !path.exists() {
        return Err(format!("Downloaded file is missing: {}", path.display()));
    }

    app_handle
        .opener()
        .open_path(path.to_string_lossy().as_ref(), None::<String>)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn trash_download_assets(
    app_handle: tauri::AppHandle,
    path: String,
    partial_paths: Vec<String>,
) -> Result<(), String> {
    let primary = authorize_download_path(&app_handle, &path, DownloadAsset::Primary)?;
    let partials = partial_paths
        .iter()
        .map(|partial| authorize_download_path(&app_handle, partial, DownloadAsset::Partial))
        .collect::<Result<Vec<_>, _>>()?;

    if primary.exists() {
        trash::delete(&primary).map_err(|e| format!("Failed to trash primary file: {}", e))?;
    }

    for partial in partials {
        if partial.exists() {
            trash::delete(&partial).map_err(|e| format!("Failed to trash partial file: {}", e))?;
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum DownloadAsset {
    Primary,
    Partial,
}

fn authorize_download_path(
    app_handle: &tauri::AppHandle,
    requested: &str,
    asset: DownloadAsset,
) -> Result<PathBuf, String> {
    let known_paths = known_download_paths(app_handle)?;
    let allowed_paths = match asset {
        DownloadAsset::Primary => known_paths,
        DownloadAsset::Partial => known_paths
            .iter()
            .flat_map(|path| [append_suffix(path, ".aria2"), append_suffix(path, ".part")])
            .collect(),
    };

    authorize_exact_path(Path::new(requested), &allowed_paths)
}

fn known_download_paths(app_handle: &tauri::AppHandle) -> Result<Vec<PathBuf>, String> {
    let store = app_handle
        .store("store.bin")
        .map_err(|e| format!("Failed to load download ownership data: {e}"))?;
    let settings = crate::settings::load_settings(app_handle).ok();
    let downloads = store
        .get("download_queue")
        .map(|value| serde_json::from_value::<Vec<crate::ipc::DownloadItem>>(value.clone()))
        .transpose()
        .map_err(|e| format!("Invalid download ownership data: {e}"))?
        .unwrap_or_default();

    Ok(downloads
        .into_iter()
        .filter_map(|download| {
            let category = format!("{:?}", download.category);
            let destination = download
                .destination
                .filter(|destination| !destination.trim().is_empty())
                .or_else(|| {
                    settings
                        .as_ref()
                        .and_then(|settings| settings.download_directories.get(&category).cloned())
                })
                .or_else(|| {
                    settings
                        .as_ref()
                        .map(|settings| settings.default_download_path.clone())
                })
                .unwrap_or_else(|| "~/Downloads".to_string());
            let filename = Path::new(&download.file_name.replace('\\', "/"))
                .file_name()?
                .to_owned();
            Some(crate::resolve_path(&destination, app_handle).join(filename))
        })
        .collect())
}

fn authorize_exact_path(requested: &Path, allowed_paths: &[PathBuf]) -> Result<PathBuf, String> {
    if std::fs::symlink_metadata(requested).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("Download path may not be a symlink".to_string());
    }

    let requested = canonicalize_with_missing_leaf(requested)?;
    if let Ok(metadata) = std::fs::metadata(&requested) {
        if !metadata.is_file() {
            return Err("Download path is not a file".to_string());
        }
    }

    let authorized = allowed_paths.iter().any(|allowed| {
        canonicalize_with_missing_leaf(allowed)
            .is_ok_and(|canonical_allowed| canonical_allowed == requested)
    });

    if authorized {
        Ok(requested)
    } else {
        Err("Path is not owned by a known download".to_string())
    }
}

fn canonicalize_with_missing_leaf(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Download path must be absolute".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err("Download path traversal is not allowed".to_string());
    }

    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| "Download path has no existing ancestor".to_string())?;
        missing.push(name.to_owned());
        existing = existing
            .parent()
            .ok_or_else(|| "Download path has no existing ancestor".to_string())?;
    }

    let mut canonical = std::fs::canonicalize(existing)
        .map_err(|e| format!("Failed to canonicalize download path: {e}"))?;
    for component in missing.iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn existing_download_asset(primary: &Path) -> Option<PathBuf> {
    [
        primary.to_path_buf(),
        append_suffix(primary, ".aria2"),
        append_suffix(primary, ".part"),
    ]
    .into_iter()
    .find(|candidate| {
        std::fs::symlink_metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    })
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::{append_suffix, authorize_exact_path};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn authorizes_only_known_download_files_and_partials() {
        let root = tempfile::tempdir().unwrap();
        let owned = root.path().join("owned.bin");
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(&owned, b"download").unwrap();

        assert_eq!(
            authorize_exact_path(&owned, std::slice::from_ref(&owned)).unwrap(),
            fs::canonicalize(&owned).unwrap()
        );
        assert!(authorize_exact_path(outside.path(), std::slice::from_ref(&owned)).is_err());

        let partial = append_suffix(&owned, ".part");
        fs::write(&partial, b"partial").unwrap();
        assert!(authorize_exact_path(&partial, std::slice::from_ref(&partial)).is_ok());
    }

    #[test]
    fn rejects_relative_traversal_and_sensitive_paths() {
        let root = tempfile::tempdir().unwrap();
        let owned = root.path().join("owned.bin");
        fs::write(&owned, b"download").unwrap();

        assert!(
            authorize_exact_path(Path::new("owned.bin"), std::slice::from_ref(&owned)).is_err()
        );
        assert!(
            authorize_exact_path(
                &root.path().join("sub/../owned.bin"),
                std::slice::from_ref(&owned)
            )
            .is_err()
        );
        assert!(
            authorize_exact_path(Path::new("/etc/hosts"), std::slice::from_ref(&owned)).is_err()
        );
        if let Some(home) = std::env::var_os("HOME") {
            assert!(
                authorize_exact_path(
                    &PathBuf::from(home).join(".ssh"),
                    std::slice::from_ref(&owned)
                )
                .is_err()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_from_download_location() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let escaped = root.path().join("owned.bin");
        symlink(outside.path().join("outside.bin"), &escaped).unwrap();

        assert!(authorize_exact_path(&escaped, std::slice::from_ref(&escaped)).is_err());
    }
}
