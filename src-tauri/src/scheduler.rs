use chrono::{Datelike, Local};
use std::time::Duration;
use tauri::Emitter;

pub fn spawn_scheduler(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;

            if let Ok(settings) = crate::settings::load_settings(&app_handle) {
                let scheduler = settings.scheduler;
                if !scheduler.enabled {
                    continue;
                }

                let now = Local::now();
                let current_time = now.format("%H:%M").to_string();
                let current_day = now.weekday().num_days_from_sunday();

                let allowed_today =
                    scheduler.everyday || scheduler.selected_days.contains(&current_day);
                if !allowed_today {
                    continue;
                }

                let date_key = now.format("%Y-%m-%d").to_string();
                let trigger_key = format!("{}-{}", date_key, current_time);

                if scheduler.start_time == current_time
                    && settings.scheduler_last_start_key != trigger_key
                {
                    let key = trigger_key.clone();
                    let _ = crate::settings::update_settings_state(&app_handle, |state| {
                        state.insert("schedulerLastStartKey".to_string(), serde_json::json!(key));
                        state.insert("schedulerRunning".to_string(), serde_json::json!(true));
                    });

                    let _ = app_handle.emit("schedule-trigger", "start");
                }

                if scheduler.stop_time_enabled
                    && scheduler.stop_time == current_time
                    && settings.scheduler_last_stop_key != trigger_key
                {
                    let key = trigger_key.clone();
                    let _ = crate::settings::update_settings_state(&app_handle, |state| {
                        state.insert("schedulerLastStopKey".to_string(), serde_json::json!(key));
                        state.insert("schedulerRunning".to_string(), serde_json::json!(false));
                    });

                    let _ = app_handle.emit("schedule-trigger", "stop");
                }
            }
        }
    });
}
