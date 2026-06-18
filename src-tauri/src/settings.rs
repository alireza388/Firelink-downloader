use crate::ipc::{
    AppFontSize, ListRowDensity, MediaCookieSource, PersistedSettings, PostQueueAction, ProxyMode,
    SchedulerSettings, SettingsTab, Theme,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const SETTINGS_STORE: &str = "store.bin";
const SETTINGS_KEY: &str = "settings";

pub fn load_settings(app_handle: &AppHandle) -> Result<PersistedSettings, String> {
    let store = app_handle
        .store(SETTINGS_STORE)
        .map_err(|error| format!("failed to open settings store: {error}"))?;
    let stored = store
        .get(SETTINGS_KEY)
        .ok_or_else(|| "settings are not persisted yet".to_string())?;
    decode_stored_settings(&stored)
}

pub fn decode_stored_settings(stored: &Value) -> Result<PersistedSettings, String> {
    let document = decode_document(stored)?;
    let state = settings_state(&document)?;
    let mut merged = serde_json::to_value(default_settings())
        .map_err(|error| format!("failed to serialize settings defaults: {error}"))?;
    merge_json(&mut merged, state);

    let mut settings: PersistedSettings = serde_json::from_value(merged)
        .map_err(|error| format!("invalid persisted settings: {error}"))?;
    validate_settings(&mut settings);
    Ok(settings)
}

pub fn update_settings_state(
    app_handle: &AppHandle,
    update: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
    let store = app_handle
        .store(SETTINGS_STORE)
        .map_err(|error| format!("failed to open settings store: {error}"))?;
    let stored = store
        .get(SETTINGS_KEY)
        .ok_or_else(|| "settings are not persisted yet".to_string())?;
    let was_string = stored.is_string();
    let mut document = decode_document(&stored)?;
    update(settings_state_mut(&mut document)?);

    let stored = if was_string {
        Value::String(
            serde_json::to_string(&document)
                .map_err(|error| format!("failed to encode settings: {error}"))?,
        )
    } else {
        document
    };
    store.set(SETTINGS_KEY, stored);
    store
        .save()
        .map_err(|error| format!("failed to save settings: {error}"))
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

fn default_settings() -> PersistedSettings {
    let download_directories = [
        ("Musics", "~/Downloads/Musics"),
        ("Movies", "~/Downloads/Movies"),
        ("Compressed", "~/Downloads/Compressed"),
        ("Documents", "~/Downloads/Documents"),
        ("Pictures", "~/Downloads/Pictures"),
        ("Applications", "~/Downloads/Applications"),
        ("Other", "~/Downloads/Other"),
    ]
    .into_iter()
    .map(|(category, path)| (category.to_string(), path.to_string()))
    .collect::<HashMap<_, _>>();

    PersistedSettings {
        theme: Theme::System,
        default_download_path: "~/Downloads".to_string(),
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
            post_queue_action: PostQueueAction::None,
        },
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
        media_cookie_source: MediaCookieSource::None,
        download_directories,
        site_logins: Vec::new(),
        extension_pairing_token: String::new(),
        auto_check_updates: true,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_stored_settings;
    use serde_json::{json, Value};

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
        assert_eq!(settings.default_download_path, "~/Downloads");
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
    fn replaces_zero_concurrency_with_the_safe_default() {
        let stored = json!({"state": {"maxConcurrentDownloads": 0}, "version": 0});

        let settings = decode_stored_settings(&Value::String(stored.to_string())).unwrap();

        assert_eq!(settings.max_concurrent_downloads, 3);
    }
}
