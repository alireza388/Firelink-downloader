use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DownloadStatus {
    Downloading,
    /// Post-download media processing such as yt-dlp/ffmpeg merging or
    /// extraction. The queue permit is still held.
    Processing,
    Paused,
    Completed,
    Failed,
    Queued,
    /// Transient state: a connection-aware retry is in progress with
    /// exponential backoff. The download slot/permit is still held.
    Retrying,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Downloading => "downloading",
            Self::Processing => "processing",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Queued => "queued",
            Self::Retrying => "retrying",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DownloadCategory {
    Musics,
    Movies,
    Compressed,
    Documents,
    Pictures,
    Applications,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Queue {
    pub id: String,
    pub name: String,
    pub is_main: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub status: DownloadStatus,
    #[ts(optional)]
    pub fraction: Option<f64>,
    #[ts(optional)]
    pub speed: Option<String>,
    #[ts(optional)]
    pub eta: Option<String>,
    #[ts(optional)]
    pub size: Option<String>,
    pub category: DownloadCategory,
    pub date_added: String,
    #[ts(optional)]
    pub connections: Option<i32>,
    #[ts(optional)]
    pub speed_limit: Option<String>,
    #[ts(optional)]
    pub username: Option<String>,
    #[ts(optional)]
    pub password: Option<String>,
    #[ts(optional)]
    pub headers: Option<String>,
    #[ts(optional)]
    pub checksum: Option<String>,
    #[ts(optional)]
    pub cookies: Option<String>,
    #[ts(optional)]
    pub mirrors: Option<String>,
    #[ts(optional)]
    pub destination: Option<String>,
    #[ts(optional)]
    pub is_media: Option<bool>,
    #[ts(optional)]
    pub media_format_selector: Option<String>,
    pub queue_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SiteLogin {
    pub id: String,
    pub url_pattern: String,
    pub username: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AppFontSize {
    Small,
    Standard,
    Large,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ListRowDensity {
    Compact,
    Standard,
    Relaxed,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum PostQueueAction {
    None,
    Sleep,
    Restart,
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Theme {
    Dark,
    Light,
    System,
    Dracula,
    Nord,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ActiveView {
    Downloads,
    Settings,
    Scheduler,
    #[serde(rename = "speedLimiter")]
    SpeedLimiter,
    Diagnostics,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum SettingsTab {
    Downloads,
    Lookandfeel,
    Network,
    Locations,
    Sitelogins,
    Power,
    Engine,
    Integrations,
    About,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ProxyMode {
    None,
    System,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum MediaCookieSource {
    None,
    Safari,
    Chrome,
    Firefox,
    Edge,
    Brave,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SchedulerSettings {
    pub enabled: bool,
    pub start_time: String,
    pub stop_time_enabled: bool,
    pub stop_time: String,
    pub everyday: bool,
    pub selected_days: Vec<u32>,
    pub post_queue_action: PostQueueAction,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct PersistedSettings {
    pub theme: Theme,
    pub default_download_path: String,
    pub max_concurrent_downloads: usize,
    pub global_speed_limit: String,
    pub is_sidebar_visible: bool,
    pub active_settings_tab: SettingsTab,
    pub scheduler: SchedulerSettings,
    pub scheduler_last_start_key: String,
    pub scheduler_last_stop_key: String,
    pub last_custom_speed_limit_ki_b: u32,
    pub per_server_connections: i32,
    pub max_automatic_retries: i32,
    pub show_notifications: bool,
    pub play_completion_sound: bool,
    pub app_font_size: AppFontSize,
    pub list_row_density: ListRowDensity,
    pub show_dock_badge: bool,
    pub show_menu_bar_icon: bool,
    pub proxy_mode: ProxyMode,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub custom_user_agent: String,
    pub ask_where_to_save_each_file: bool,
    pub prevents_sleep_while_downloading: bool,
    pub media_cookie_source: MediaCookieSource,
    pub download_directories: HashMap<String, String>,
    pub site_logins: Vec<SiteLogin>,
    // Note: `extension_pairing_token` is intentionally NOT persisted here. It
    // is an HMAC shared secret and is stored in the OS keychain by the
    // frontend. The field is kept on legacy persisted JSON only; serde ignores
    // unknown fields when decoding, so existing installs migrate cleanly.
    pub auto_check_updates: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum QueueDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadStateEvent {
    pub id: String,
    pub status: String,
    pub error: Option<String>,
}

impl DownloadStateEvent {
    pub fn new(id: impl Into<String>, status: DownloadStatus) -> Self {
        Self {
            id: id.into(),
            status: status.as_str().to_string(),
            error: None,
        }
    }

    pub fn failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: DownloadStatus::Failed.as_str().to_string(),
            error: Some(error.into()),
        }
    }

    /// Transient retry state. Carries the human-readable reason so the UI can
    /// surface "network dropped, retrying in 5s…". The slot is still held.
    pub fn retrying(id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: DownloadStatus::Retrying.as_str().to_string(),
            error: Some(reason.into()),
        }
    }
}
