use crate::ipc::{
    AppFontSize, ListRowDensity, MediaCookieSource, PersistedSettings, PostQueueAction, ProxyMode,
    SchedulerSettings, SettingsTab, Theme,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

pub fn load_settings<R: tauri::Runtime>(
    app_handle: &AppHandle<R>,
) -> Result<PersistedSettings, String> {
    let database = app_handle.state::<crate::db::DbState>();
    let connection = database.lock()?;
    let stored = crate::db::load_settings(&connection)?
        .ok_or_else(|| "settings are not persisted yet".to_string())?;
    decode_stored_settings(&Value::String(stored))
}

pub fn decode_stored_settings(stored: &Value) -> Result<PersistedSettings, String> {
    let document = decode_document(stored)?;
    let mut state = settings_state(&document)?.clone();
    migrate_location_settings(&mut state)?;
    let mut merged = serde_json::to_value(default_settings())
        .map_err(|error| format!("failed to serialize settings defaults: {error}"))?;
    merge_json(&mut merged, &state);

    let mut settings: PersistedSettings = serde_json::from_value(merged)
        .map_err(|error| format!("invalid persisted settings: {error}"))?;
    validate_settings(&mut settings);
    Ok(settings)
}

pub fn update_settings_state(
    app_handle: &AppHandle,
    update: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
    let database = app_handle.state::<crate::db::DbState>();
    let connection = database.lock()?;
    let stored = crate::db::load_settings(&connection)?
        .ok_or_else(|| "settings are not persisted yet".to_string())?;
    let mut document = decode_document(&Value::String(stored))?;
    update(settings_state_mut(&mut document)?);
    let stored = serde_json::to_string(&document)
        .map_err(|error| format!("failed to encode settings: {error}"))?;
    crate::db::save_settings(&connection, &stored)
}

pub fn preserve_scheduler_runtime_keys(
    existing: Option<&str>,
    incoming: &str,
) -> Result<String, String> {
    let Some(existing) = existing else {
        return Ok(incoming.to_string());
    };
    let existing_document = match decode_document(&Value::String(existing.to_string())) {
        Ok(doc) => doc,
        Err(e) => {
            log::warn!(
                "Failed to decode existing settings, dropping runtime keys: {}",
                e
            );
            return Ok(incoming.to_string());
        }
    };
    let existing_state = match settings_state(&existing_document) {
        Ok(state) => state,
        Err(_) => return Ok(incoming.to_string()),
    };
    let mut incoming_document = decode_document(&Value::String(incoming.to_string()))?;
    let incoming_state = settings_state_mut(&mut incoming_document)?;
    for key in ["schedulerLastStartKey", "schedulerLastStopKey"] {
        if let Some(value) = existing_state.get(key) {
            incoming_state.insert(key.to_string(), value.clone());
        }
    }
    serde_json::to_string(&incoming_document)
        .map_err(|error| format!("failed to encode persisted settings: {error}"))
}

fn decode_document(stored: &Value) -> Result<Value, String> {
    match stored {
        Value::String(text) => serde_json::from_str(text)
            .map_err(|error| format!("failed to parse persisted settings: {error}")),
        Value::Object(_) => Ok(stored.clone()),
        _ => Err("persisted settings must be a JSON string or object".to_string()),
    }
}

fn settings_state(document: &Value) -> Result<&Value, String> {
    if let Some(state) = document.get("state") {
        if !state.is_object() {
            return Err("persisted settings state must be an object".to_string());
        }
        Ok(state)
    } else if document.is_object() {
        Ok(document)
    } else {
        Err("persisted settings must be an object".to_string())
    }
}

fn settings_state_mut(document: &mut Value) -> Result<&mut Map<String, Value>, String> {
    let has_envelope = document.get("state").is_some();
    let state = if has_envelope {
        document
            .get_mut("state")
            .ok_or_else(|| "persisted settings state is missing".to_string())?
    } else {
        document
    };
    state
        .as_object_mut()
        .ok_or_else(|| "persisted settings state must be an object".to_string())
}

fn merge_json(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                if let Some(target_value) = target.get_mut(key) {
                    merge_json(target_value, value);
                } else {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn validate_settings(settings: &mut PersistedSettings) {
    if settings.max_concurrent_downloads == 0 {
        settings.max_concurrent_downloads = default_settings().max_concurrent_downloads;
    }
}

fn default_category_subfolders() -> HashMap<String, String> {
    [
        ("Musics", "Musics"),
        ("Movies", "Movies"),
        ("Compressed", "Compressed"),
        ("Documents", "Documents"),
        ("Pictures", "Pictures"),
        ("Applications", "Applications"),
        ("Other", "Other"),
    ]
    .into_iter()
    .map(|(category, folder)| (category.to_string(), folder.to_string()))
    .collect()
}

fn normalize_category_subfolder(value: &str, fallback: &str) -> String {
    let parts = value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !part.ends_with(':'))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        fallback.to_string()
    } else {
        parts.join("/")
    }
}

fn migrate_location_settings(state: &mut Value) -> Result<(), String> {
    let state = state
        .as_object_mut()
        .ok_or_else(|| "persisted settings state must be an object".to_string())?;
    let base = state
        .get("baseDownloadFolder")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            state
                .get("defaultDownloadPath")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("~/Downloads")
        .to_string();

    let mut subfolders = default_category_subfolders();
    if let Some(persisted) = state.get("categorySubfolders").and_then(Value::as_object) {
        for (category, value) in persisted {
            if let Some(folder) = value.as_str().filter(|folder| !folder.trim().is_empty()) {
                let fallback = subfolders
                    .get(category)
                    .cloned()
                    .unwrap_or_else(|| category.clone());
                subfolders.insert(
                    category.clone(),
                    normalize_category_subfolder(folder, &fallback),
                );
            }
        }
    }

    let mut overrides = state
        .get("categoryDirectoryOverrides")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(legacy) = state.get("downloadDirectories").and_then(Value::as_object) {
        let aliases = [
            ("Musics", "Audio"),
            ("Movies", "Video"),
            ("Compressed", "Archives"),
            ("Documents", "Documents"),
            ("Pictures", "Images"),
            ("Applications", "Apps"),
            ("Other", "Other"),
        ];
        for (category, alias) in aliases {
            if overrides.contains_key(category) {
                continue;
            }
            let Some(path) = legacy
                .get(category)
                .or_else(|| legacy.get(alias))
                .and_then(Value::as_str)
                .filter(|path| !path.trim().is_empty())
            else {
                continue;
            };
            let subfolder = subfolders
                .get(category)
                .map(String::as_str)
                .unwrap_or(category);
            if normalize_location_path(path) != derived_location_path(&base, subfolder) {
                overrides.insert(category.to_string(), Value::String(path.to_string()));
            }
        }
    }

    state.insert("baseDownloadFolder".to_string(), Value::String(base));
    state.insert(
        "categorySubfolders".to_string(),
        serde_json::to_value(subfolders)
            .map_err(|error| format!("failed to migrate category subfolders: {error}"))?,
    );
    state.insert(
        "categoryDirectoryOverrides".to_string(),
        Value::Object(overrides),
    );
    state.remove("defaultDownloadPath");
    state.remove("downloadDirectories");
    Ok(())
}

fn normalize_location_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

fn derived_location_path(base: &str, subfolder: &str) -> String {
    format!(
        "{}/{}",
        normalize_location_path(base),
        subfolder.trim_matches(|character| character == '/' || character == '\\')
    )
}

fn default_settings() -> PersistedSettings {
    PersistedSettings {
        theme: Theme::System,
        base_download_folder: "~/Downloads".to_string(),
        category_subfolders: default_category_subfolders(),
        category_directory_overrides: HashMap::new(),
        approved_download_roots: Vec::new(),
        max_concurrent_downloads: 3,
        global_speed_limit: String::new(),
        is_sidebar_visible: true,
        active_settings_tab: SettingsTab::Downloads,
        scheduler: SchedulerSettings {
            enabled: false,
            start_time: "00:00".to_string(),
            stop_time_enabled: false,
            stop_time: "08:00".to_string(),
            everyday: true,
            selected_days: vec![0, 1, 2, 3, 4, 5, 6],
            selected_queue_ids: vec!["00000000-0000-0000-0000-000000000001".to_string()],
            post_queue_action: PostQueueAction::None,
        },
        scheduler_running: false,
        scheduler_active_download_ids: Vec::new(),
        scheduler_last_start_key: String::new(),
        scheduler_last_stop_key: String::new(),
        last_custom_speed_limit_ki_b: 1024,
        per_server_connections: 16,
        max_automatic_retries: 3,
        show_notifications: true,
        play_completion_sound: true,
        app_font_size: AppFontSize::Standard,
        list_row_density: ListRowDensity::Standard,
        show_dock_badge: true,
        show_menu_bar_icon: true,
        proxy_mode: ProxyMode::None,
        proxy_host: String::new(),
        proxy_port: 8080,
        custom_user_agent: String::new(),
        ask_where_to_save_each_file: false,
        prevents_sleep_while_downloading: true,
        media_cookie_source: MediaCookieSource::default(),
        site_logins: Vec::new(),
        extension_pairing_token: String::new(),
        auto_check_updates: true,
        keychain_access_granted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_stored_settings, preserve_scheduler_runtime_keys};
    use serde_json::{json, Value};

    #[test]
    fn frontend_settings_save_preserves_backend_scheduler_keys() {
        let existing = json!({
            "state": {
                "schedulerLastStartKey": "2026-06-22-start",
                "schedulerLastStopKey": "2026-06-22-stop"
            },
            "version": 3
        })
        .to_string();
        let incoming = json!({
            "state": {
                "schedulerLastStartKey": "",
                "schedulerLastStopKey": "",
                "theme": "system"
            },
            "version": 3
        })
        .to_string();

        let merged = preserve_scheduler_runtime_keys(Some(&existing), &incoming).unwrap();
        let merged: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(merged["state"]["schedulerLastStartKey"], "2026-06-22-start");
        assert_eq!(merged["state"]["schedulerLastStopKey"], "2026-06-22-stop");
    }

    #[test]
    fn decodes_zustand_envelope_and_preserves_non_default_startup_settings() {
        let stored = json!({
            "state": {
                "maxConcurrentDownloads": 7,
                "globalSpeedLimit": "2M",
                "scheduler": {
                    "enabled": true,
                    "startTime": "06:30",
                    "stopTimeEnabled": true,
                    "stopTime": "23:15",
                    "everyday": false,
                    "selectedDays": [1, 3, 5],
                    "postQueueAction": "sleep"
                }
            },
            "version": 0
        });

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.max_concurrent_downloads, 7);
        assert_eq!(settings.global_speed_limit, "2M");
        assert!(settings.scheduler.enabled);
        assert_eq!(settings.scheduler.start_time, "06:30");
        assert_eq!(settings.scheduler.selected_days, vec![1, 3, 5]);
        assert_eq!(
            settings.scheduler.selected_queue_ids,
            vec!["00000000-0000-0000-0000-000000000001"]
        );
        assert_eq!(settings.base_download_folder, "~/Downloads");
    }

    #[test]
    fn decodes_legacy_top_level_settings() {
        let stored = json!({
            "maxConcurrentDownloads": 5,
            "globalSpeedLimit": "512K"
        });

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.max_concurrent_downloads, 5);
        assert_eq!(settings.global_speed_limit, "512K");
        assert!(!settings.scheduler.enabled);
    }

    #[test]
    fn migrates_legacy_location_settings_and_preserves_custom_overrides() {
        let stored = json!({
            "state": {
                "defaultDownloadPath": "/Users/test/Downloads",
                "downloadDirectories": {
                    "Movies": "/Users/test/Downloads/Movies",
                    "Documents": "/Volumes/Archive/Documents"
                }
            },
            "version": 1
        });

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.base_download_folder, "/Users/test/Downloads");
        assert_eq!(settings.category_subfolders["Movies"], "Movies");
        assert!(!settings.category_directory_overrides.contains_key("Movies"));
        assert_eq!(
            settings.category_directory_overrides["Documents"],
            "/Volumes/Archive/Documents"
        );
    }

    #[test]
    fn normalizes_category_subfolders_as_relative_paths() {
        let stored = json!({
            "state": {
                "baseDownloadFolder": "/Users/test/Downloads",
                "categorySubfolders": {
                    "Movies": "../Media/./Movies",
                    "Documents": "../../"
                }
            },
            "version": 2
        });

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.category_subfolders["Movies"], "Media/Movies");
        assert_eq!(settings.category_subfolders["Documents"], "Documents");
    }

    #[test]
    fn replaces_zero_concurrency_with_the_safe_default() {
        let stored = json!({"state": {"maxConcurrentDownloads": 0}, "version": 0});

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.max_concurrent_downloads, 3);
    }

    #[test]
    fn ignores_legacy_extension_pairing_token_field() {
        // Older versions persisted `extensionPairingToken` as plaintext inside
        // the settings document. It now lives in the OS keychain and is no
        // longer part of PersistedSettings. serde ignores the unknown field so
        // existing installs decode without error; the plaintext value is
        // simply dropped and a fresh token is minted by the frontend.
        let stored = json!({
            "state": {
                "extensionPairingToken": "plaintext-leaked-secret",
                "maxConcurrentDownloads": 5
            },
            "version": 0
        });

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.max_concurrent_downloads, 5);
    }
}
