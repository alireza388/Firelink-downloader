use chrono::{Datelike, Local, Timelike};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::Emitter;

fn minute_of_day(value: &str) -> Option<u32> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

fn stop_is_due(
    stop_time_enabled: bool,
    stop_minute: Option<u32>,
    current_minute: u32,
    last_start_key: &str,
    start_key: &str,
    last_stop_key: &str,
    stop_key: &str,
) -> bool {
    stop_time_enabled
        && stop_minute.is_some_and(|stop| current_minute >= stop)
        && last_start_key == start_key
        && last_stop_key != stop_key
}

pub fn spawn_scheduler(
    app_handle: tauri::AppHandle,
    settings_cache: Arc<RwLock<Option<crate::ipc::PersistedSettings>>>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_emit: HashMap<&'static str, std::time::Instant> = HashMap::new();
        loop {
            interval.tick().await;

            let settings = settings_cache.read().ok().and_then(|settings| {
                settings.as_ref().map(|settings| {
                    (
                        settings.scheduler.clone(),
                        settings.scheduler_last_start_key.clone(),
                        settings.scheduler_last_stop_key.clone(),
                    )
                })
            });
            if let Some((scheduler, scheduler_last_start_key, scheduler_last_stop_key)) = settings {
                if !scheduler.enabled {
                    continue;
                }

                let now = Local::now();
                let current_minute = now.hour() * 60 + now.minute();
                let current_day = now.weekday().num_days_from_sunday();

                let allowed_today =
                    scheduler.everyday || scheduler.selected_days.contains(&current_day);
                if !allowed_today {
                    continue;
                }

                let date_key = now.format("%Y-%m-%d").to_string();
                let start_key = format!("{date_key}-start");
                let stop_key = format!("{date_key}-stop");
                let start_minute = minute_of_day(&scheduler.start_time);
                let stop_minute = minute_of_day(&scheduler.stop_time);
                let before_stop = !scheduler.stop_time_enabled
                    || stop_minute.is_some_and(|stop| current_minute < stop);

                if start_minute.is_some_and(|start| current_minute >= start)
                    && before_stop
                    && scheduler_last_start_key != start_key
                    && last_emit
                        .get("start")
                        .is_none_or(|instant| instant.elapsed() >= Duration::from_secs(5))
                {
                    let _ = app_handle.emit(
                        "schedule-trigger",
                        serde_json::json!({
                            "action": "start",
                            "key": start_key
                        }),
                    );
                    last_emit.insert("start", std::time::Instant::now());
                }

                if stop_is_due(
                    scheduler.stop_time_enabled,
                    stop_minute,
                    current_minute,
                    &scheduler_last_start_key,
                    &start_key,
                    &scheduler_last_stop_key,
                    &stop_key,
                ) && last_emit
                    .get("stop")
                    .is_none_or(|instant| instant.elapsed() >= Duration::from_secs(5))
                {
                    let _ = app_handle.emit(
                        "schedule-trigger",
                        serde_json::json!({
                            "action": "stop",
                            "key": stop_key
                        }),
                    );
                    last_emit.insert("stop", std::time::Instant::now());
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{minute_of_day, stop_is_due};

    #[test]
    fn parses_valid_scheduler_times() {
        assert_eq!(minute_of_day("00:00"), Some(0));
        assert_eq!(minute_of_day("23:59"), Some(1439));
        assert_eq!(minute_of_day("06:30"), Some(390));
    }

    #[test]
    fn rejects_invalid_scheduler_times() {
        assert_eq!(minute_of_day("24:00"), None);
        assert_eq!(minute_of_day("12:60"), None);
        assert_eq!(minute_of_day("bad"), None);
    }

    #[test]
    fn stop_requires_same_day_acknowledged_start() {
        assert!(!stop_is_due(
            true,
            Some(480),
            600,
            "",
            "2026-06-22-start",
            "",
            "2026-06-22-stop",
        ));
        assert!(stop_is_due(
            true,
            Some(480),
            600,
            "2026-06-22-start",
            "2026-06-22-start",
            "",
            "2026-06-22-stop",
        ));
    }
}
