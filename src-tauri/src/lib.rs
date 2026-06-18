#![allow(unexpected_cfgs)]

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::{Manager, Emitter};
use regex::Regex;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;
use tauri_plugin_deep_link::DeepLinkExt;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MetadataResponse {
    filename: String,
    size: String,
    #[ts(type = "number")]
    size_bytes: u64,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MediaFormat {
    pub format_id: String,
    pub resolution: String,
    pub ext: String,
    #[ts(type = "number | null")]
    pub fps: Option<f64>,
    #[ts(type = "number | null")]
    pub filesize: Option<u64>,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MediaMetadata {
    pub title: String,
    #[ts(type = "number | null")]
    pub duration: Option<u64>,
    pub thumbnail: Option<String>,
    pub formats: Vec<MediaFormat>,
}

fn is_media_processing_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("[merger]")
        || lower.contains("[extractaudio]")
        || lower.contains("[ffmpeg]")
        || lower.contains("[videoconvertor]")
        || lower.contains("[fixup")
        || lower.contains("merging formats")
        || lower.contains("post-process")
}

async fn cleanup_media_processing_artifacts(out_path: &std::path::Path) {
    let Some(parent) = out_path.parent() else {
        return;
    };
    let Some(base_name) = out_path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let base_stem = out_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(base_name);

    let _ = tokio::fs::remove_file(out_path).await;

    let Ok(mut entries) = tokio::fs::read_dir(parent).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path == out_path {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(base_name) && !name.starts_with(base_stem) {
            continue;
        }
        let yt_dlp_format_fragment = name
            .strip_prefix(base_stem)
            .and_then(|suffix| suffix.strip_prefix(".f"))
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(|ch| ch.is_ascii_digit());
        let looks_like_media_temp = name.contains(".part")
            || name.contains(".ytdl")
            || name.contains(".temp")
            || name.contains(".tmp")
            || yt_dlp_format_fragment;
        if looks_like_media_temp {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}




#[tauri::command]
async fn fetch_metadata(url: String, user_agent: Option<String>, username: Option<String>, password: Option<String>) -> Result<MetadataResponse, String> {
    let mut current_url = url.clone();
    let mut redirects = 0;
    let res;
    
    loop {
        if redirects >= 5 {
            return Err("Too many redirects".to_string());
        }
        
        let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
        
        if let Some(ref ua) = user_agent {
            if !ua.is_empty() {
                builder = builder.user_agent(ua);
            } else {
                builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
            }
        } else {
            builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
        }

        let mut resolved_addr = None;
        if let Ok(parsed) = reqwest::Url::parse(&current_url) {
            if let Some(host) = parsed.host_str() {
                let port = parsed.port_or_known_default().unwrap_or(80);
                if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host, port)) {
                    if let Some(addr) = addrs.into_iter().next() {
                        let ip = addr.ip();
                        if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
                            return Err("SSRF blocked: Private/local IP not allowed".to_string());
                        }
                        if let std::net::IpAddr::V4(ipv4) = ip {
                            if ipv4.is_private() || ipv4.is_link_local() {
                                return Err("SSRF blocked: Private/local IP not allowed".to_string());
                            }
                        }
                        resolved_addr = Some((host.to_string(), addr));
                    }
                }
            }
        }

        if let Some((host, addr)) = resolved_addr {
            builder = builder.resolve(&host, addr);
        }

        let client = builder.build().map_err(|e| e.to_string())?;

        let mut head_req = client.head(&current_url);
        if let Some(ref user) = username {
            if !user.is_empty() {
                head_req = head_req.basic_auth(user, password.as_deref());
            }
        }
        let mut current_res = head_req.send().await.map_err(|e| e.to_string())?;

        if !current_res.status().is_success() && !current_res.status().is_redirection() {
            let mut get_req = client.get(&current_url);
            if let Some(ref user) = username {
                if !user.is_empty() {
                    get_req = get_req.basic_auth(user, password.as_deref());
                }
            }
            current_res = get_req.send().await.map_err(|e| e.to_string())?;
        }

        if current_res.status().is_redirection() {
            if let Some(loc) = current_res.headers().get(reqwest::header::LOCATION) {
                if let Ok(loc_str) = loc.to_str() {
                    if let Ok(parsed_base) = reqwest::Url::parse(&current_url) {
                        if let Ok(new_url) = parsed_base.join(loc_str) {
                            current_url = new_url.to_string();
                            redirects += 1;
                            continue;
                        }
                    }
                }
            }
        }

        res = current_res;
        break;
    }

    let mut filename = String::new();
    if let Some(cd) = res.headers().get(reqwest::header::CONTENT_DISPOSITION) {
        if let Ok(cd_str) = cd.to_str() {
            if let Some(idx) = cd_str.find("filename=") {
                let rest = &cd_str[idx + 9..];
                let raw_filename = rest.trim_matches(|c| c == '"' || c == '\'');
                let normalized = raw_filename.replace('\\', "/");
                filename = std::path::Path::new(&normalized)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("download")
                    .to_string();
            }
        }
    }

    if filename.is_empty() {
        if let Ok(parsed) = reqwest::Url::parse(&current_url) {
            if let Some(mut segments) = parsed.path_segments() {
                if let Some(last) = segments.next_back() {
                    let normalized = last.replace('\\', "/");
                    filename = std::path::Path::new(&normalized)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("download")
                        .to_string();
                }
            }
        }
    }
    if filename.is_empty() {
        filename = "download".to_string();
    }

    let mut size_str = "Unknown".to_string();
    let mut size_bytes = 0;
    if let Some(len) = res.headers().get(reqwest::header::CONTENT_LENGTH) {
        if let Ok(len_str) = len.to_str() {
            if let Ok(bytes) = len_str.parse::<u64>() {
                size_bytes = bytes;
                if bytes < 1024 {
                    size_str = format!("{} B", bytes);
                } else if bytes < 1024 * 1024 {
                    size_str = format!("{:.1} KB", bytes as f64 / 1024.0);
                } else if bytes < 1024 * 1024 * 1024 {
                    size_str = format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0);
                } else {
                    size_str = format!("{:.2} GB", bytes as f64 / 1024.0 / 1024.0 / 1024.0);
                }
            }
        }
    }

    Ok(MetadataResponse { filename, size: size_str, size_bytes })
}

#[tauri::command]
async fn fetch_media_metadata(app_handle: tauri::AppHandle, url: String, cookie_browser: Option<String>, username: Option<String>, password: Option<String>) -> Result<MediaMetadata, String> {
    println!("fetch_media_metadata called for: {}", url);
    
    // Resolve bundled deno and ffmpeg binaries and create a temporary PATH for yt-dlp
    let deno_path = resolve_bundled_binary_path(&app_handle, "deno").map_err(|e| format!("failed to find bundled deno: {e}"))?;
    let ffmpeg_path = resolve_bundled_binary_path(&app_handle, "ffmpeg").map_err(|e| format!("failed to find bundled ffmpeg: {e}"))?;
    
    let bin_dir = tempfile::tempdir().map_err(|e| format!("failed to create bundling temp dir: {e}"))?;
    {
        use std::os::unix::fs::symlink;
        symlink(&deno_path, bin_dir.path().join("deno")).map_err(|e| format!("failed to symlink deno: {e}"))?;
        symlink(&ffmpeg_path, bin_dir.path().join("ffmpeg")).map_err(|e| format!("failed to symlink ffmpeg: {e}"))?;
    }
    let bin_dir_str = bin_dir.path().to_string_lossy().to_string();
    let path_env = format!("{}:/usr/bin:/bin", bin_dir_str);

    use tauri_plugin_shell::ShellExt;
    let mut cmd = app_handle.shell().sidecar("yt-dlp").map_err(|e| format!("Failed to create sidecar yt-dlp: {}", e))?;
    cmd = cmd.env("PATH", &path_env)
       .arg("--dump-json")
       .arg("--no-warnings")
       .arg("--no-playlist")
       .arg("--socket-timeout").arg("20")
       .arg("--retries").arg("3")
       .arg("--extractor-retries").arg("3")
       .arg("--compat-options").arg("no-youtube-unavailable-videos");

    if let Some(browser) = cookie_browser {
        if !browser.is_empty() {
            cmd = cmd.arg("--cookies-from-browser").arg(&browser);
        }
    }
    
    let mut config_file = tempfile::Builder::new().prefix("ytdlp-").suffix(".conf").tempfile().map_err(|e| e.to_string())?;
    let mut config_content = String::new();
    if let Some(user) = username {
        if !user.is_empty() {
            config_content.push_str(&format!("--username\n{}\n", user));
        }
    }
    if let Some(pass) = password {
        if !pass.is_empty() {
            config_content.push_str(&format!("--password\n{}\n", pass));
        }
    }
    use std::io::Write;
    config_file.write_all(config_content.as_bytes()).map_err(|e| e.to_string())?;
    let config_path = config_file.into_temp_path();
    if !config_content.is_empty() {
        cmd = cmd.arg("--config-location").arg(&config_path);
    }

    cmd = cmd.arg("--").arg(&url);

    let output = cmd.output()
        .await
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if output.status.success() {
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse JSON: {}", e))?;
        
        let title = value.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown Title").to_string();
        let duration = value.get("duration").and_then(|v| v.as_f64()).map(|v| v as u64);
        let thumbnail = value.get("thumbnail").and_then(|v| v.as_str()).map(|s| s.to_string());
        
        let mut formats = Vec::new();
        if let Some(formats_arr) = value.get("formats").and_then(|v| v.as_array()) {
            for fmt in formats_arr {
                let format_id = fmt.get("format_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let resolution = fmt.get("resolution").and_then(|v| v.as_str()).unwrap_or("audio only").to_string();
                let ext = fmt.get("ext").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let fps = fmt.get("fps").and_then(|v| v.as_f64());
                let filesize = fmt.get("filesize").and_then(|v| v.as_u64()).or_else(|| fmt.get("filesize_approx").and_then(|v| v.as_f64().map(|f| f as u64)));
                
                if !format_id.is_empty() {
                    formats.push(MediaFormat { format_id, resolution, ext, fps, filesize });
                }
            }
        }
        
        Ok(MediaMetadata { title, duration, thumbnail, formats })
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("yt-dlp error: {}", err))
    }
}

#[tauri::command]
async fn test_ytdlp(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("test_ytdlp called!");
    use tauri_plugin_shell::ShellExt;
    
    let output = app_handle.shell().sidecar("yt-dlp")
        .map_err(|e| e.to_string())?
        .arg("--version")
        .output()
        .await
        .map_err(|e| {
            println!("Failed to execute: {}", e);
            format!("Failed to execute yt-dlp: {}", e)
        })?;

    println!("yt-dlp execution finished with status: {:?}", output.status);
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("yt-dlp output: {}", text);
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        println!("yt-dlp error output: {}", err);
        Err(format!("yt-dlp error: {}", err))
    }
}

#[tauri::command]
async fn test_ffmpeg(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("test_ffmpeg called!");
    use tauri_plugin_shell::ShellExt;

    let output = app_handle.shell().sidecar("ffmpeg")
        .map_err(|e| e.to_string())?
        .arg("-version")
        .output()
        .await
        .map_err(|e| {
            println!("Failed to execute: {}", e);
            format!("Failed to execute ffmpeg: {}", e)
        })?;

    println!("ffmpeg execution finished with status: {:?}", output.status);
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let first_line = text.lines().next().unwrap_or("");
        let re = regex::Regex::new(r"(?i)version\s+([\d\.]+)").unwrap();
        let clean = re.captures(first_line)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| {
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                parts.get(2).unwrap_or(&first_line).split('-').next().unwrap_or("").to_string()
            });
        println!("ffmpeg output: {}", clean);
        Ok(clean)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        println!("ffmpeg error output: {}", err);
        Err(format!("ffmpeg error: {}", err))
    }
}

#[tauri::command]
async fn test_deno(app_handle: tauri::AppHandle) -> Result<String, String> {
    println!("test_deno called!");
    use tauri_plugin_shell::ShellExt;

    let output = app_handle.shell().sidecar("deno")
        .map_err(|e| e.to_string())?
        .arg("--version")
        .output()
        .await
        .map_err(|e| {
            println!("Failed to execute: {}", e);
            format!("Failed to execute deno: {}", e)
        })?;

    println!("deno execution finished with status: {:?}", output.status);
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let re = regex::Regex::new(r"deno\s+(\d+\.\d+\.\d+)").unwrap();
        let clean = re.captures(&text).and_then(|c| c.get(1)).map(|m| m.as_str()).unwrap_or(&text).to_string();
        println!("deno output: {}", clean);
        Ok(clean)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        println!("deno error output: {}", err);
        Err(format!("deno error: {}", err))
    }
}

pub(crate) fn is_safe_path(path: &std::path::Path, app_handle: &tauri::AppHandle) -> bool {
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }

    if !path.is_absolute() {
        return true;
    }

    let mut allowed_prefixes = Vec::new();
    use tauri::Manager;
    if let Ok(home) = app_handle.path().home_dir() {
        allowed_prefixes.push(home.join("Downloads"));
        allowed_prefixes.push(home.join("Music"));
        allowed_prefixes.push(home.join("Movies"));
        allowed_prefixes.push(home.join("Pictures"));
        allowed_prefixes.push(home.join("Documents"));
        allowed_prefixes.push(home.join("Desktop"));
    }
    allowed_prefixes.push(std::path::PathBuf::from("/Volumes"));

    for prefix in allowed_prefixes {
        if path.starts_with(&prefix) {
            return true;
        }
    }

    false
}

#[tauri::command]
async fn open_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    println!("open_file called for path: {}", path);
    use tauri_plugin_opener::OpenerExt;
    
    let resolved_dest = resolve_path(&path, &app);
    
    if !is_safe_path(&resolved_dest, &app) {
        return Err("Path traversal blocked".to_string());
    }

    app.opener().open_path(resolved_dest.to_string_lossy().as_ref(), None::<String>).map_err(|e| format!("Failed to open file: {}", e))
}

#[tauri::command]
async fn show_in_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    println!("show_in_folder called for path: {}", path);
    use tauri_plugin_opener::OpenerExt;
    
    let resolved_dest = resolve_path(&path, &app);
    
    if !is_safe_path(&resolved_dest, &app) {
        return Err("Path traversal blocked".to_string());
    }

    app.opener().reveal_item_in_dir(resolved_dest.to_string_lossy().as_ref()).map_err(|e| format!("Failed to reveal in folder: {}", e))
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

struct Aria2DaemonGuard {
    child: Mutex<Option<std::process::Child>>,
    startup_error: Mutex<Option<String>>,
    last_stderr: Mutex<String>,
}

impl Aria2DaemonGuard {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            startup_error: Mutex::new(None),
            last_stderr: Mutex::new(String::new()),
        }
    }
}

impl Drop for Aria2DaemonGuard {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}



pub mod download;
pub mod queue;
#[allow(dead_code)]
pub mod ipc;
mod parity;
pub mod error;
pub mod commands;
pub mod retry;
mod settings;
pub use error::AppError;

// Retained only for compatibility with the optional aria2 diagnostic monitor.
// Active downloads are owned by DownloadCoordinator.
#[non_exhaustive]
pub enum TaskHandle {
    Aria2(String),
}

pub struct AppState {
    pub download_coordinator: download::DownloadCoordinator,
    pub extension_pairing_token: extension_server::SharedExtensionToken,
    pub extension_frontend_ready: extension_server::SharedFrontendReady,
    pub extension_server_shutdown: tokio::sync::watch::Sender<bool>,
    pub aria2_port: u16,
    pub aria2_secret: String,
    pub media_semaphore: Arc<tokio::sync::Semaphore>,
    pub sleep_preventer: Arc<Mutex<Option<keepawake::KeepAwake>>>,
    pub queue_manager: Arc<queue::QueueManager>,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadProgressEvent {
    id: String,
    fraction: f64,
    speed: String,
    eta: String,
    size: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct EngineStatusItem {
    name: String,
    kind: String,
    expected_sidecar: String,
    resolved_path: Option<String>,
    version: Option<String>,
    ready: bool,
    error: Option<String>,
    stderr_tail: Option<String>,
    remediation_hint: Option<String>,
    rpc_port: Option<u16>,
    daemon_alive: Option<bool>,
    rpc_ready: Option<bool>,
    last_stderr_tail: Option<String>,
    expects_internal_dir: Option<bool>,
    has_internal_dir: Option<bool>,
    has_python_framework: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct EngineStatusResult {
    pub engines: Vec<EngineStatusItem>,
}


pub(crate) fn resolve_path(path: &str, app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;
    let mut resolved = std::path::PathBuf::from(path);
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = app_handle.path().home_dir() {
            resolved = home.join(stripped);
        }
    } else if path == "~" {
        if let Ok(home) = app_handle.path().home_dir() {
            resolved = home;
        }
    }
    resolved
}

pub(crate) fn collect_download_uris(url: &str, mirrors: Option<&str>) -> Vec<String> {
    let mut uris = Vec::new();
    for uri in std::iter::once(url).chain(mirrors.into_iter().flat_map(str::lines)) {
        let uri = uri.trim();
        if !uri.is_empty() && !uris.iter().any(|existing| existing == uri) {
            uris.push(uri.to_string());
        }
    }
    uris
}

const MAX_DEEP_LINK_PAYLOAD_LEN: usize = 65_536;
const MAX_DEEP_LINK_URLS: usize = 200;

fn parse_firelink_urls(deep_links: impl IntoIterator<Item = url::Url>) -> Vec<String> {
    let mut captured = Vec::new();

    for deep_link in deep_links {
        if deep_link.scheme() != "firelink" || deep_link.host_str() != Some("add") {
            continue;
        }

        let Some(raw_urls) = deep_link
            .query_pairs()
            .find_map(|(key, value)| (key == "url").then(|| value.into_owned()))
        else {
            continue;
        };
        if raw_urls.is_empty() || raw_urls.chars().count() >= MAX_DEEP_LINK_PAYLOAD_LEN {
            continue;
        }

        for raw_url in raw_urls.lines() {
            let raw_url = raw_url.trim();
            let Ok(url) = url::Url::parse(raw_url) else {
                continue;
            };
            if !matches!(url.scheme(), "http" | "https" | "ftp" | "sftp") {
                continue;
            }
            let url = url.to_string();
            if !captured.iter().any(|existing| existing == &url) {
                captured.push(url);
                if captured.len() == MAX_DEEP_LINK_URLS {
                    return captured;
                }
            }
        }
    }

    captured
}

fn restore_main_window(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn dispatch_deep_links(app_handle: tauri::AppHandle, deep_links: Vec<url::Url>) {
    let urls = parse_firelink_urls(deep_links);
    if urls.is_empty() {
        return;
    }

    restore_main_window(&app_handle);
    let coordinator = app_handle.state::<AppState>().download_coordinator.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = coordinator
            .send(download::DownloadCmd::CaptureUrls(urls))
            .await
        {
            eprintln!("Failed to dispatch deep link to download coordinator: {error}");
        }
    });
}

pub(crate) async fn rpc_call(port: u16, secret: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("http://127.0.0.1:{}/jsonrpc", port);
    let mut payload = serde_json::Map::new();
    payload.insert("jsonrpc".to_string(), serde_json::json!("2.0"));
    payload.insert("id".to_string(), serde_json::json!("1"));
    payload.insert("method".to_string(), serde_json::json!(method));
    
    let mut p = vec![serde_json::json!(format!("token:{}", secret))];
    if let serde_json::Value::Array(arr) = params {
        p.extend(arr);
    }
    payload.insert("params".to_string(), serde_json::json!(p));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    if let Some(error) = json.get("error") {
        return Err(error.to_string());
    }
    json.get("result")
        .cloned()
        .ok_or_else(|| "aria2 returned no result".to_string())
}

#[tauri::command]
async fn test_aria2c(app_handle: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = app_handle.state::<Aria2DaemonGuard>();
    let startup_err = guard.startup_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(err) = startup_err {
        return Err(format!("aria2 daemon unavailable: {err}"));
    }

    let result = rpc_call(
        state.aria2_port,
        &state.aria2_secret,
        "aria2.getVersion",
        serde_json::json!([]),
    )
    .await
    .map_err(|error| format!("aria2 daemon unavailable: {error}"))?;

    result
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "aria2 returned an invalid version response".to_string())
}

// ── get_engine_status: Structured engine diagnostics ──────────────

async fn run_sidecar_version(
    app_handle: &tauri::AppHandle,
    sidecar_name: &str,
    args: &[&str],
) -> (Option<String>, Option<String>, Option<String>) {
    let binary_path = match resolve_bundled_binary_path(app_handle, sidecar_name) {
        Ok(p) => p,
        Err(e) => return (None, Some(format!("Cannot find '{}': {}", sidecar_name, e)), None),
    };

    let bin = binary_path.clone();
    let arg_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::process::Command::new(&bin)
            .args(&arg_owned)
            .output().await
    }).await;

    let output = match result {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return (None, Some(format!("Failed to execute '{}': {}", sidecar_name, e)), None),
        Err(_) => return (None, Some(format!("'{}' version check timed out after 5 seconds", sidecar_name)), None),
    };

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stderr_tail = if stderr.is_empty() { None } else { Some(stderr.clone()) };

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (Some(stdout), None, stderr_tail)
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let err = if !stderr.is_empty() {
            stderr.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        } else {
            format!("Exited with code {:?}", output.status.code())
        };
        (if stdout.is_empty() { None } else { Some(stdout) }, Some(err), stderr_tail)
    }
}

fn generate_remediation_hint(error: &str, _kind: &str) -> Option<String> {
    let lower = error.to_lowercase();
    if lower.contains("library not loaded") || lower.contains("dylib") {
        Some("A required system library is missing. Try reinstalling Firelink or run 'brew install openssl'.".to_string())
    } else if lower.contains("not found") || lower.contains("could not find") {
        Some("The bundled binary file is missing. Reinstall Firelink to restore it.".to_string())
    } else if lower.contains("timed out") {
        Some("The binary did not respond within the timeout. It may be damaged or incompatible with this system.".to_string())
    } else if lower.contains("permission denied") {
        Some("The binary does not have execute permission. Try reinstalling Firelink.".to_string())
    } else {
        None
    }
}

fn arch_suffix() -> &'static str {
    if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" }
}

async fn check_aria2(app_handle: &tauri::AppHandle, port: u16, secret: &str) -> EngineStatusItem {
    let sidecar_name = "aria2c";
    let expected_sidecar = format!("{}-{}-apple-darwin", sidecar_name, arch_suffix());

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved.as_ref().ok().map(|p| p.to_string_lossy().to_string());

    let (startup_err, daemon_stderr) = {
        let guard = app_handle.state::<Aria2DaemonGuard>();
        let se = guard.startup_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let stderr = guard.last_stderr.lock().unwrap_or_else(|e| e.into_inner()).clone();
        (se, stderr)
    };
    let daemon_alive = startup_err.is_none();
    let last_stderr_tail = if daemon_stderr.is_empty() { None } else { Some(daemon_stderr) };

    let (version_raw, run_error, stderr_tail) = run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
    let version = version_raw.and_then(|v| v.lines().next().map(|l| l.trim().to_string()));

    let rpc_ready = if daemon_alive {
        rpc_call(port, secret, "aria2.getVersion", serde_json::json!([]))
            .await
            .is_ok()
    } else {
        false
    };

    let error = startup_err.or(run_error);
    let ready = daemon_alive && rpc_ready && version.is_some();
    let remediation_hint = error.as_ref().and_then(|e| generate_remediation_hint(e, sidecar_name));

    EngineStatusItem {
        name: "Aria2".to_string(),
        kind: "aria2".to_string(),
        expected_sidecar,
        resolved_path,
        version,
        ready,
        error,
        stderr_tail,
        remediation_hint,
        rpc_port: Some(port),
        daemon_alive: Some(daemon_alive),
        rpc_ready: Some(rpc_ready),
        last_stderr_tail,
        expects_internal_dir: None,
        has_internal_dir: None,
        has_python_framework: None,
    }
}

async fn check_ytdlp(app_handle: &tauri::AppHandle) -> EngineStatusItem {
    let sidecar_name = "yt-dlp";
    let expected_sidecar = format!("{}-{}-apple-darwin", sidecar_name, arch_suffix());

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved.as_ref().ok().map(|p| p.to_string_lossy().to_string());

    let (has_internal_dir, has_python_framework) = if let Some(ref path) = resolved_path {
        let parent = std::path::Path::new(path).parent().map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            let internal = parent.join("_internal");
            let hi = internal.is_dir();
            let hp = if hi {
                internal.join("Python.framework").is_dir() || internal.join("Python").exists()
            } else {
                false
            };
            (hi, hp)
        } else {
            (false, false)
        }
    } else {
        (false, false)
    };

    let (version_raw, run_error, stderr_tail) = run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
    let version = version_raw.and_then(|v| v.lines().next().map(|l| l.trim().to_string()));

    let mut error = run_error;
    let mut remediation_hint = None;

    if error.is_none() && has_internal_dir && !has_python_framework {
        error = Some("_internal/Python.framework was not found beside yt-dlp sidecar".to_string());
        remediation_hint = Some("The yt-dlp distribution is missing its embedded Python runtime. Reinstall Firelink.".to_string());
    }

    if remediation_hint.is_none() {
        remediation_hint = error.as_ref().and_then(|e| generate_remediation_hint(e, sidecar_name));
    }

    EngineStatusItem {
        name: "yt-dlp".to_string(),
        kind: "ytdlp".to_string(),
        expected_sidecar,
        resolved_path,
        version,
        ready: error.is_none(),
        error,
        stderr_tail,
        remediation_hint,
        rpc_port: None,
        daemon_alive: None,
        rpc_ready: None,
        last_stderr_tail: None,
        expects_internal_dir: Some(has_internal_dir),
        has_internal_dir: Some(has_internal_dir),
        has_python_framework: Some(has_python_framework),
    }
}

async fn check_ffmpeg(app_handle: &tauri::AppHandle) -> EngineStatusItem {
    let sidecar_name = "ffmpeg";
    let expected_sidecar = format!("{}-{}-apple-darwin", sidecar_name, arch_suffix());

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved.as_ref().ok().map(|p| p.to_string_lossy().to_string());

    let (version_raw, run_error, stderr_tail) = run_sidecar_version(app_handle, sidecar_name, &["-version"]).await;
    let version = version_raw.as_ref().and_then(|text| {
        text.lines().next().and_then(|first| {
            let re = regex::Regex::new(r"(?i)version\s+([\d\.]+)").unwrap();
            if let Some(caps) = re.captures(first) {
                caps.get(1).map(|m| m.as_str().to_string())
            } else {
                let parts: Vec<&str> = first.split_whitespace().collect();
                parts.get(2).map(|v| v.split('-').next().unwrap_or(v).to_string())
            }
        })
    });

    let error = run_error;
    let remediation_hint = error.as_ref().and_then(|e| generate_remediation_hint(e, sidecar_name));

    EngineStatusItem {
        name: "FFmpeg".to_string(),
        kind: "ffmpeg".to_string(),
        expected_sidecar,
        resolved_path,
        version,
        ready: error.is_none(),
        error,
        stderr_tail,
        remediation_hint,
        rpc_port: None,
        daemon_alive: None,
        rpc_ready: None,
        last_stderr_tail: None,
        expects_internal_dir: None,
        has_internal_dir: None,
        has_python_framework: None,
    }
}

async fn check_deno(app_handle: &tauri::AppHandle) -> EngineStatusItem {
    let sidecar_name = "deno";
    let expected_sidecar = format!("{}-{}-apple-darwin", sidecar_name, arch_suffix());

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved.as_ref().ok().map(|p| p.to_string_lossy().to_string());

    let (version_raw, run_error, stderr_tail) = run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
    let version = version_raw.as_ref().and_then(|text| {
        let re = regex::Regex::new(r"deno\s+(\d+\.\d+\.\d+)").ok()?;
        re.captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
    }).or(version_raw);

    let error = run_error;
    let remediation_hint = error.as_ref().and_then(|e| generate_remediation_hint(e, sidecar_name));

    EngineStatusItem {
        name: "Deno".to_string(),
        kind: "deno".to_string(),
        expected_sidecar,
        resolved_path,
        version,
        ready: error.is_none(),
        error,
        stderr_tail,
        remediation_hint,
        rpc_port: None,
        daemon_alive: None,
        rpc_ready: None,
        last_stderr_tail: None,
        expects_internal_dir: None,
        has_internal_dir: None,
        has_python_framework: None,
    }
}

#[tauri::command]
async fn get_engine_status(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<EngineStatusResult, String> {
    let port = state.aria2_port;
    let secret = state.aria2_secret.clone();

    let (aria2, ytdlp, ffmpeg, deno) = tokio::join!(
        check_aria2(&app_handle, port, &secret),
        check_ytdlp(&app_handle),
        check_ffmpeg(&app_handle),
        check_deno(&app_handle),
    );

    Ok(EngineStatusResult {
        engines: vec![aria2, ytdlp, ffmpeg, deno],
    })
}


fn resolve_bundled_binary_path(app_handle: &tauri::AppHandle, binary_name: &str) -> Result<std::path::PathBuf, String> {
    let full_name = format!(
        "{}-{}-apple-darwin",
        binary_name,
        if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" },
    );

    // Production: sidecar sits next to the main executable inside the .app bundle
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join(&full_name);
            if candidate.is_file() {
                log::info!("Resolved bundled '{}' at: {:?}", binary_name, candidate);
                return Ok(candidate);
            }
        }
    }

    // Dev mode: search relative to CWD
    if let Ok(cwd) = std::env::current_dir() {
        let search_dirs = [
            cwd.join("binaries"),
            cwd.join("src-tauri").join("binaries"),
        ];
        for dir in &search_dirs {
            let candidate = dir.join(&full_name);
            if candidate.is_file() {
                let abs = candidate.canonicalize().map_err(|e| {
                    format!("Failed to canonicalize '{}': {}", full_name, e)
                })?;
                log::info!("Resolved bundled '{}' at: {:?}", binary_name, abs);
                return Ok(abs);
            }
        }
    }

    // Fallback: Tauri resource directory (some configs place sidecars there)
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        let candidate = resource_dir.join(&full_name);
        if candidate.is_file() {
            log::info!("Resolved bundled '{}' at: {:?}", binary_name, candidate);
            return Ok(candidate);
        }
        let candidate2 = resource_dir.join("binaries").join(&full_name);
        if candidate2.is_file() {
            log::info!("Resolved bundled '{}' at: {:?}", binary_name, candidate2);
            return Ok(candidate2);
        }
    }

    Err(format!(
        "Could not find bundled binary '{}' (expected name: {})",
        binary_name, full_name
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_media_download_internal(
    app_handle: tauri::AppHandle,
    id: &str,
    url: String,
    destination: String,
    filename: String,
    format_selector: Option<String>,
    cookie_source: Option<String>,
    speed_limit: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Option<String>,
    proxy: Option<String>,
    user_agent: Option<String>,
    max_tries: Option<i32>,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    println!("start_media_download called for id: {}", id);
    let safe_filename = std::path::Path::new(&filename.replace('\\', "/"))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download")
        .to_string();


    let resolved_dest = resolve_path(&destination, &app_handle);

    if !is_safe_path(&resolved_dest, &app_handle) {
        return Err("Path traversal blocked".to_string());
    }

    if !resolved_dest.exists() {
        let _ = tokio::fs::create_dir_all(&resolved_dest).await;
    }

    let out_path = resolved_dest.join(&safe_filename);

    let total_tracks: f64 = if let Some(ref format) = format_selector {
        if format.contains('+') { 2.0 } else { 1.0 }
    } else {
        1.0
    };

    use tauri_plugin_shell::ShellExt;

    let mut config_file = tempfile::Builder::new().prefix("ytdlp-").suffix(".conf").tempfile().map_err(|e| e.to_string())?;
    let mut config_content = String::new();
    if let Some(user) = username {
        if !user.is_empty() {
            config_content.push_str(&format!("--username\n{}\n", user));
        }
    }
    if let Some(pass) = password {
        if !pass.is_empty() {
            config_content.push_str(&format!("--password\n{}\n", pass));
        }
    }
    if let Some(headers) = headers {
        for header in headers.lines().map(str::trim).filter(|header| !header.is_empty()) {
            config_content.push_str(&format!("--add-header\n{}\n", header));
        }
    }
    use std::io::Write;
    config_file.write_all(config_content.as_bytes()).map_err(|e| e.to_string())?;
    let config_path = config_file.into_temp_path();

    use crate::ipc::DownloadStateEvent;
    use crate::retry::{BackoffOutcome, MAX_RETRIES, backoff_and_emit_cancel, is_transient_network_error};

    const STDERR_TAIL: usize = 2048;

    let config_location = if !config_content.is_empty() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    };
    let _keep_alive = config_path;
    let mut current_track: f64 = 0.0;
    let mut last_fraction: f64 = 0.0;
    let mut last_progress_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(200))
        .unwrap_or_else(std::time::Instant::now);

    static PCT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SPD_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ETA_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let pct_re = PCT_RE.get_or_init(|| Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap());
    let spd_re = SPD_RE.get_or_init(|| Regex::new(r"at\s+([^\s]+)").unwrap());
    let eta_re = ETA_RE.get_or_init(|| Regex::new(r"ETA\s+([^\s]+)").unwrap());

    // Resolve absolute paths to bundled binaries
    let aria2c_path = resolve_bundled_binary_path(&app_handle, "aria2c")?;
    let ffmpeg_path = resolve_bundled_binary_path(&app_handle, "ffmpeg")?;
    let deno_path = resolve_bundled_binary_path(&app_handle, "deno")?;
    log::info!("Using bundled aria2c: {:?}", aria2c_path);
    log::info!("Using bundled ffmpeg: {:?}", ffmpeg_path);
    log::info!("Using bundled deno: {:?}", deno_path);

    // Create a temp directory with bare-name symlinks so yt-dlp finds the
    // bundled binaries via PATH when told --downloader aria2c (bare name).
    let bin_dir = tempfile::tempdir().map_err(|e| format!("failed to create bundling temp dir: {e}"))?;
    {
        use std::os::unix::fs::symlink;
        symlink(&aria2c_path, bin_dir.path().join("aria2c"))
            .map_err(|e| format!("failed to symlink aria2c: {e}"))?;
        symlink(&ffmpeg_path, bin_dir.path().join("ffmpeg"))
            .map_err(|e| format!("failed to symlink ffmpeg: {e}"))?;
        symlink(&deno_path, bin_dir.path().join("deno"))
            .map_err(|e| format!("failed to symlink deno: {e}"))?;
    }
    let bin_dir_str = bin_dir.path().to_string_lossy().to_string();
    // Minimal PATH: bundled dir first, then only essential system paths.
    // No user-writable or Homebrew paths that could shadow our binaries.
    let path_env = format!("{}:/usr/bin:/bin", bin_dir_str);

    let mut strike = 0_usize;
    let mut processing_started = false;

    while strike <= MAX_RETRIES {
        let mut cmd = app_handle.shell().sidecar("yt-dlp").map_err(|e| e.to_string())?
           .arg("--newline")
           .arg("--no-check-formats")
           .arg("--socket-timeout").arg("20")
           .arg("--retries").arg("3")
           .arg("--extractor-retries").arg("3")
           .arg("--downloader").arg("aria2c")
           .arg("--downloader-args").arg("aria2c:-c -x 16 -s 16 -k 1M")
           .arg("--ffmpeg-location").arg(&bin_dir_str)
           .arg("--concurrent-fragments").arg("4")
           .arg("--no-warnings")
           .arg("--continue")
           .arg("--compat-options").arg("no-youtube-unavailable-videos")
           .arg("-o").arg(out_path.to_string_lossy().to_string())
           .env("PATH", &path_env);

        if let Some(limit) = speed_limit.as_ref() {
            if !limit.is_empty() {
                cmd = cmd.arg("--limit-rate").arg(limit);
            }
        }

        if let Some(p) = proxy.as_ref() {
            if !p.is_empty() {
                cmd = cmd.arg("--proxy").arg(p);
            }
        }

        if let Some(cs) = cookie_source.as_ref() {
            let mut cs = cs.clone();
            if !cs.is_empty() && cs != "none" {
                if cs == "safari" { cs = "safari:".to_string() }
                cmd = cmd.arg("--cookies-from-browser").arg(cs);
            }
        }

        if let Some(ua) = user_agent.as_ref() {
            if !ua.is_empty() {
                cmd = cmd.arg("--user-agent").arg(ua);
            }
        }

        if let Some(tries) = max_tries {
            cmd = cmd.arg("--retries").arg(tries.to_string());
        }

        if let Some(loc) = config_location.as_ref() {
            cmd = cmd.arg("--config-location").arg(loc);
        }

        if let Some(format) = format_selector.as_ref() {
            cmd = cmd.arg("-f").arg(format);
            if safe_filename.ends_with(".mp3") {
                cmd = cmd.arg("-x").arg("--audio-format").arg("mp3");
            } else if safe_filename.ends_with(".m4a") {
                cmd = cmd.arg("-x").arg("--audio-format").arg("m4a");
            } else if safe_filename.ends_with(".opus") {
                cmd = cmd.arg("-x").arg("--audio-format").arg("opus");
            } else if safe_filename.ends_with(".mp4") {
                cmd = cmd.arg("--merge-output-format").arg("mp4");
            } else if safe_filename.ends_with(".webm") {
                cmd = cmd.arg("--merge-output-format").arg("webm");
            } else {
                cmd = cmd.arg("--merge-output-format").arg("mkv");
            }
        }

        cmd = cmd.arg("--").arg(&url);

        let (mut rx, child) = cmd.spawn().map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;
        log::info!("yt-dlp spawned for id: {} (strike {})", id, strike);

        let mut stderr_tail = String::new();
        let failure_reason = loop {
            tokio::select! {
                _ = cancel_rx.changed() => {
                    let _ = child.kill();
                    if processing_started {
                        cleanup_media_processing_artifacts(&out_path).await;
                    }
                    return Err(crate::queue::MEDIA_RUN_CANCELLED.to_string());
                }
                event = rx.recv() => {
                    match event {
                        Some(tauri_plugin_shell::process::CommandEvent::Stdout(line_bytes)) => {
                            let line = String::from_utf8_lossy(&line_bytes);
                            if line.contains("[download]") && line.contains("%") {
                                let fraction = if let Some(cap) = pct_re.captures(&line) {
                                    cap.get(1).and_then(|m| m.as_str().parse::<f64>().ok()).unwrap_or(0.0) / 100.0
                                } else {
                                    0.0
                                };

                                if fraction < last_fraction && (last_fraction - fraction) > 0.5 {
                                    current_track += 1.0;
                                }
                                last_fraction = fraction;

                                let overall_fraction = ((current_track + fraction) / total_tracks).min(1.0);

                                let speed = if let Some(cap) = spd_re.captures(&line) {
                                    cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "-".to_string())
                                } else {
                                    "-".to_string()
                                };

                                let eta = if let Some(cap) = eta_re.captures(&line) {
                                    cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| "-".to_string())
                                } else {
                                    "-".to_string()
                                };

                                let now = std::time::Instant::now();
                                if now.duration_since(last_progress_at) >= std::time::Duration::from_millis(200) {
                                    let _ = app_handle.emit("download-progress", DownloadProgressEvent {
                                        id: id.to_string(),
                                        fraction: overall_fraction,
                                        speed,
                                        eta,
                                        size: None,
                                    });
                                    last_progress_at = now;
                                }
                            }
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Stderr(line_bytes)) => {
                            let line = String::from_utf8_lossy(&line_bytes);
                            if !processing_started && is_media_processing_line(&line) {
                                processing_started = true;
                                let _ = app_handle.emit(
                                    "download-state",
                                    DownloadStateEvent::new(
                                        id,
                                        crate::ipc::DownloadStatus::Processing,
                                    ),
                                );
                                let _ = app_handle.emit("download-progress", DownloadProgressEvent {
                                    id: id.to_string(),
                                    fraction: 1.0,
                                    speed: "Processing".to_string(),
                                    eta: "-".to_string(),
                                    size: None,
                                });
                            }
                            let lower = line.to_lowercase();
                            if lower.contains("error") || lower.contains("critical") {
                                log::error!("yt-dlp stderr [{}]: {}", id, line.trim());
                            }
                            stderr_tail.push_str(&line);
                            if stderr_tail.len() > STDERR_TAIL {
                                stderr_tail = stderr_tail.split_off(stderr_tail.len() - STDERR_TAIL);
                            }
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Error(err)) => {
                            log::error!("yt-dlp shell error [{}]: {}", id, err);
                            break err;
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Terminated(payload)) => {
                            if payload.code == Some(0) {
                                log::info!("yt-dlp completed successfully for id: {}", id);
                                return Ok(());
                            }
                            log::error!("yt-dlp exited with non-zero code {:?} for id: {}", payload.code, id);
                            break if stderr_tail.is_empty() {
                                format!("yt-dlp exited with code {:?}", payload.code)
                            } else {
                                stderr_tail.clone()
                            };
                        }
                        Some(_) => {}
                        None => {
                            break if stderr_tail.is_empty() {
                                "yt-dlp process ended unexpectedly".to_string()
                            } else {
                                stderr_tail.clone()
                            };
                        }
                    }
                }
            }
        };

        let transient = is_transient_network_error(&failure_reason);
        let strikes_left = strike < MAX_RETRIES;
        if !(transient && strikes_left) {
            return Err(failure_reason);
        }

        let reason = failure_reason.clone();
        let outcome = backoff_and_emit_cancel(
            strike,
            reason,
            cancel_rx,
            |retry_reason| {
                let _ = app_handle.emit(
                    "download-state",
                    DownloadStateEvent::retrying(id, retry_reason),
                );
            },
        )
        .await;

        if outcome == BackoffOutcome::Aborted {
            return Err(crate::queue::MEDIA_RUN_CANCELLED.to_string());
        }

        strike += 1;
    }

    Err("yt-dlp retry loop exhausted".to_string())
}

#[tauri::command]
async fn pause_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    log::info!("pause_download called for id: {}", id);

    let active_kind = state.queue_manager.active_kind(&id).await;
    state.queue_manager.remove_from_pending(&id).await;

    let gid = state.queue_manager.aria2_gid_for_download(&id);
    if let Some(gid) = gid.as_deref().filter(|gid| !gid.starts_with("native:")) {
        let status = aria2_download_status(
            state.aria2_port,
            &state.aria2_secret,
            gid,
        )
        .await?;
        match status.as_str() {
            "paused" => {
                log::info!("aria2 pause [{}]: gid {} was already paused", id, gid);
            }
            "active" | "waiting" => {
                let result = rpc_call(
                    state.aria2_port,
                    &state.aria2_secret,
                    "aria2.forcePause",
                    serde_json::json!([gid]),
                )
                .await
                .map_err(|error| format!("failed to pause aria2 gid {gid}: {error}"))?;
                ensure_aria2_gid_result("forcePause", gid, &result)?;
                log::info!("aria2 pause [{}]: gid {} paused", id, gid);
            }
            terminal => {
                state.queue_manager.clear_aria2_retry_state(&id).await;
                state.queue_manager.forget_aria2_gid(&id).await;
                state.queue_manager.release_permit(&id).await;
                return Err(format!(
                    "cannot pause aria2 gid {gid} in terminal state {terminal}"
                ));
            }
        }

        state.queue_manager.release_permit(&id).await;
        use tauri::Emitter;
        let _ = app_handle.emit(
            "download-state",
            crate::ipc::DownloadStateEvent::new(id, crate::ipc::DownloadStatus::Paused),
        );
        return Ok(());
    }

    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id.clone(), crate::ipc::DownloadStatus::Paused),
    );

    if let Ok(download_id) = Uuid::parse_str(&id) {
        let _ = state
            .download_coordinator
            .send(download::DownloadCmd::Pause(download_id))
            .await;
    }
    let media_result = state.download_coordinator.pause_media(id.clone()).await;
    if !matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
        state.queue_manager.release_permit(&id).await;
    }
    media_result
}

#[tauri::command]
async fn resume_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let Some(gid) = state.queue_manager.aria2_gid_for_download(&id) else {
        log::info!("aria2 resume [{}]: no mapped gid; re-enqueue is permitted", id);
        return Ok(false);
    };
    if gid.starts_with("native:") {
        state.queue_manager.forget_aria2_gid(&id).await;
        log::info!("aria2 resume [{}]: native fallback has no aria2 gid", id);
        return Ok(false);
    }

    let status = aria2_download_status(state.aria2_port, &state.aria2_secret, &gid).await?;
    match status.as_str() {
        "paused" => {
            let acquired = state.queue_manager.ensure_aria2_permit(&id).await;
            let result = match rpc_call(
                state.aria2_port,
                &state.aria2_secret,
                "aria2.unpause",
                serde_json::json!([gid]),
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    if acquired {
                        state.queue_manager.release_permit(&id).await;
                    }
                    return Err(format!("failed to resume aria2 gid {gid}: {error}"));
                }
            };
            if let Err(error) = ensure_aria2_gid_result("unpause", &gid, &result) {
                if acquired {
                    state.queue_manager.release_permit(&id).await;
                }
                return Err(error);
            }
            log::info!("aria2 resume [{}]: unpaused gid {}", id, gid);
        }
        "active" | "waiting" => {
            state.queue_manager.ensure_aria2_permit(&id).await;
            log::info!(
                "aria2 resume [{}]: gid {} already {}; no duplicate job created",
                id,
                gid,
                status
            );
        }
        "complete" | "error" | "removed" => {
            state.queue_manager.clear_aria2_retry_state(&id).await;
            state.queue_manager.forget_aria2_gid(&id).await;
            state.queue_manager.release_permit(&id).await;
            log::info!(
                "aria2 resume [{}]: gid {} is {}; re-enqueue is permitted",
                id,
                gid,
                status
            );
            return Ok(false);
        }
        other => {
            return Err(format!("aria2 gid {gid} returned unknown status {other}"));
        }
    }

    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id, crate::ipc::DownloadStatus::Downloading),
    );
    Ok(true)
}

#[tauri::command]
async fn remove_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    filepath: Option<String>,
) -> Result<(), String> {
    log::info!("remove_download called for id: {}", id);

    let active_kind = state.queue_manager.active_kind(&id).await;
    state.queue_manager.remove_from_pending(&id).await;

    state.queue_manager.cancel_aria2_retries(&id).await;
    let retry_add_guard = state.queue_manager.lock_aria2_retry_add().await;
    let gid = state.queue_manager.aria2_gid_for_download(&id);
    if let Some(gid) = gid.as_deref().filter(|gid| !gid.starts_with("native:")) {
        let removal_result = async {
            force_remove_aria2_gid(state.aria2_port, &state.aria2_secret, gid).await?;
            wait_for_aria2_stopped(state.aria2_port, &state.aria2_secret, gid).await
        }
        .await;
        if let Err(error) = removal_result {
            state.queue_manager.allow_aria2_retries(&id).await;
            return Err(error);
        }
        state.queue_manager.clear_aria2_retry_state(&id).await;
        state.queue_manager.forget_aria2_gid(&id).await;
        state.queue_manager.release_permit(&id).await;
        log::info!("aria2 remove [{}]: gid {} stopped and forgotten", id, gid);
    } else {
        drop(retry_add_guard);
        if let Ok(download_id) = Uuid::parse_str(&id) {
            state
                .download_coordinator
                .send(download::DownloadCmd::Cancel(download_id))
                .await?;
        }
        state.download_coordinator.pause_media(id.clone()).await?;
        if !matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
            state.queue_manager.release_permit(&id).await;
        }
        state.queue_manager.clear_aria2_retry_state(&id).await;
        state.queue_manager.forget_aria2_gid(&id).await;
    }

    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id.clone(), crate::ipc::DownloadStatus::Paused),
    );

    if let Some(path) = filepath {
        if !path.is_empty() {
            let p = std::path::Path::new(&path);
            if is_safe_path(p, &app_handle) {
                if p.exists() {
                    let _ = tokio::fs::remove_file(p).await;
                }
                let aria2_path = format!("{}.aria2", path);
                let p_aria2 = std::path::Path::new(&aria2_path);
                if p_aria2.exists() {
                    let _ = tokio::fs::remove_file(p_aria2).await;
                }
            }
        }
    }

    Ok(())
}

fn ensure_aria2_gid_result(
    method: &str,
    expected_gid: &str,
    result: &serde_json::Value,
) -> Result<(), String> {
    match result.as_str() {
        Some(returned_gid) if returned_gid == expected_gid => Ok(()),
        Some(returned_gid) => Err(format!(
            "aria2.{method} returned unexpected gid {returned_gid}, expected {expected_gid}"
        )),
        None => Err(format!("aria2.{method} returned a non-string result")),
    }
}

async fn aria2_download_status(port: u16, secret: &str, gid: &str) -> Result<String, String> {
    let result = rpc_call(
        port,
        secret,
        "aria2.tellStatus",
        serde_json::json!([gid, ["status"]]),
    )
    .await
    .map_err(|error| format!("failed to query aria2 gid {gid}: {error}"))?;
    result
        .get("status")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("aria2.tellStatus returned no status for gid {gid}"))
}

fn aria2_gid_not_found(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("gid") && lower.contains("not found")
}

async fn force_remove_aria2_gid(port: u16, secret: &str, gid: &str) -> Result<(), String> {
    match rpc_call(
        port,
        secret,
        "aria2.forceRemove",
        serde_json::json!([gid]),
    )
    .await
    {
        Ok(result) => ensure_aria2_gid_result("forceRemove", gid, &result),
        Err(error) if aria2_gid_not_found(&error) => {
            log::info!("aria2 forceRemove: gid {} was already absent", gid);
            Ok(())
        }
        Err(error) => match aria2_download_status(port, secret, gid).await {
            Ok(status) if matches!(status.as_str(), "complete" | "error" | "removed") => {
                log::info!(
                    "aria2 forceRemove: gid {} raced to terminal state {}",
                    gid,
                    status
                );
                Ok(())
            }
            _ => Err(format!("failed to remove aria2 gid {gid}: {error}")),
        },
    }
}

async fn wait_for_aria2_stopped(port: u16, secret: &str, gid: &str) -> Result<(), String> {
    for _ in 0..30 {
        match aria2_download_status(port, secret, gid).await {
            Ok(status) if matches!(status.as_str(), "complete" | "error" | "removed") => {
                return Ok(());
            }
            Ok(_) => {}
            Err(error) if aria2_gid_not_found(&error) => return Ok(()),
            Err(error) => return Err(error),
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "aria2 gid {gid} did not stop within 3 seconds after forceRemove"
    ))
}

#[tauri::command]
fn update_dock_badge(_app_handle: tauri::AppHandle, count: i32) {
    #[cfg(target_os = "macos")]
    {
        use cocoa::appkit::NSApp;
        use cocoa::base::{nil, id};
        use cocoa::foundation::NSString;
        use objc::{msg_send, sel, sel_impl};
        
        unsafe {
            let app = NSApp();
            let dock_tile: id = msg_send![app, dockTile];
            let label = if count > 0 { count.to_string() } else { "".to_string() };
            let ns_label = NSString::alloc(nil).init_str(&label);
            let _: () = msg_send![dock_tile, setBadgeLabel: ns_label];
        }
    }
}

#[tauri::command]
fn set_prevent_sleep(state: tauri::State<'_, AppState>, prevent: bool) {
    let mut current_preventer = state.sleep_preventer.lock().unwrap_or_else(|e| e.into_inner());
    if prevent {
        if current_preventer.is_none() {
            if let Ok(keepawake) = keepawake::Builder::default().display(true).reason("Downloading files").create() {
                *current_preventer = Some(keepawake);
            }
        }
    } else {
        *current_preventer = None;
    }
}

pub(crate) fn execute_system_action(action: crate::ipc::PostQueueAction) -> Result<(), String> {
    match action {
        crate::ipc::PostQueueAction::Shutdown => {
            system_shutdown::shutdown().map_err(|e| e.to_string())
        }
        crate::ipc::PostQueueAction::Restart => {
            system_shutdown::reboot().map_err(|e| e.to_string())
        }
        crate::ipc::PostQueueAction::Sleep => {
            system_shutdown::sleep().map_err(|e| e.to_string())
        }
        crate::ipc::PostQueueAction::None => Err("Invalid action".to_string()),
    }
}

#[tauri::command]
fn perform_system_action(action: crate::ipc::PostQueueAction) -> Result<(), String> {
    execute_system_action(action)
}

#[tauri::command]
async fn get_pending_order(state: tauri::State<'_, AppState>) -> Result<Vec<String>, AppError> {
    Ok(state.queue_manager.pending_order().await)
}

#[tauri::command]
async fn enqueue_download(
    state: tauri::State<'_, AppState>,
    item: queue::EnqueueItem,
) -> Result<String, AppError> {
    let id = item.id.clone();
    state.queue_manager.push(item.into_task()).await;
    Ok(id)
}

#[tauri::command]
async fn enqueue_many(
    state: tauri::State<'_, AppState>,
    items: Vec<queue::EnqueueItem>,
) -> Result<(), AppError> {
    let tasks = items.into_iter().map(queue::EnqueueItem::into_task).collect();
    state.queue_manager.enqueue_many(tasks).await;
    Ok(())
}

#[tauri::command]
async fn move_in_queue(
    state: tauri::State<'_, AppState>,
    id: String,
    direction: crate::ipc::QueueDirection,
) -> Result<Vec<String>, AppError> {
    Ok(state.queue_manager.move_in_queue(&id, direction).await)
}

#[tauri::command]
async fn remove_from_queue(state: tauri::State<'_, AppState>, id: String) -> Result<bool, AppError> {
    Ok(state.queue_manager.remove_from_pending(&id).await)
}

#[tauri::command]
async fn set_concurrent_limit(state: tauri::State<'_, AppState>, limit: usize) -> Result<(), String> {
    state.queue_manager.set_capacity(limit);
    Ok(())
}

#[tauri::command]
async fn set_global_speed_limit(state: tauri::State<'_, AppState>, limit: Option<String>) -> Result<(), String> {
    let limit_str = limit.unwrap_or_else(|| "0".to_string());
    rpc_call(
        state.aria2_port,
        &state.aria2_secret,
        "aria2.changeGlobalOption",
        serde_json::json!([{"max-overall-download-limit": limit_str}])
    ).await.map(|_| ()).map_err(|e| {
        eprintln!("Failed to set global speed limit: {}", e);
        e
    })
}

#[tauri::command]
fn request_automation_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use cocoa::foundation::NSString;
        use cocoa::base::{nil, id};
        use objc::{msg_send, sel, sel_impl, class};

        unsafe {
            objc::rc::autoreleasepool(|| {
                let script_str = NSString::alloc(nil).init_str("tell application \"Finder\" to get name");
                let ns_apple_script: id = msg_send![class!(NSAppleScript), alloc];
                let ns_apple_script: id = msg_send![ns_apple_script, initWithSource: script_str];
                let mut error_dict: id = nil;
                let result: id = msg_send![ns_apple_script, executeAndReturnError: &mut error_dict];
                if result == nil {
                    return Err("Automation permission was not granted".to_string());
                }
                Ok(())
            })
        }
    }

    #[cfg(not(target_os = "macos"))]
    Ok(())
}

#[tauri::command]
fn open_automation_settings(app_handle: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        app_handle.opener().open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation", None::<String>)
            .map_err(|e| format!("Failed to open Automation settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    Err("Automation settings are only available on macOS".to_string())
}

#[tauri::command]
fn get_free_space(app_handle: tauri::AppHandle, path: String) -> Result<String, String> {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();

    let resolved_dest = resolve_path(&path, &app_handle);

    // Find the disk that the path is mounted on
    let mut best_match: Option<&sysinfo::Disk> = None;
    let mut max_match_len = 0;

    for disk in disks.list() {
        let mount_point = disk.mount_point();
        if resolved_dest.starts_with(mount_point) {
            let match_len = mount_point.as_os_str().len();
            if match_len > max_match_len {
                max_match_len = match_len;
                best_match = Some(disk);
            }
        }
    }

    if let Some(disk) = best_match {
        let bytes = disk.available_space();
        let size_str = if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
        } else {
            format!("{:.2} GB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
        };
        Ok(size_str)
    } else {
        Ok("Unknown".to_string())
    }
}

#[tauri::command]
fn set_keychain_password(id: String, password: String) -> Result<(), String> {
    let entry = keyring::Entry::new("com.firelink.app", &id).map_err(|e| e.to_string())?;
    entry.set_password(&password).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_keychain_password(id: String) -> Result<String, String> {
    let entry = keyring::Entry::new("com.firelink.app", &id).map_err(|e| e.to_string())?;
    entry.get_password().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_keychain_password(id: String) -> Result<(), String> {
    let entry = keyring::Entry::new("com.firelink.app", &id).map_err(|e| e.to_string())?;
    let _ = entry.delete_credential(); // Ignore error if it doesn't exist
    Ok(())
}

#[tauri::command]
fn check_file_exists(app_handle: tauri::AppHandle, path: String) -> bool {
    let resolved_dest = resolve_path(&path, &app_handle);
    if !is_safe_path(&resolved_dest, &app_handle) {
        return false;
    }
    resolved_dest.exists()
}

#[tauri::command]
fn delete_file(app_handle: tauri::AppHandle, path: String) -> Result<(), String> {
    let resolved_dest = resolve_path(&path, &app_handle);
    if !is_safe_path(&resolved_dest, &app_handle) {
        return Err("Path traversal blocked".to_string());
    }
    if resolved_dest.exists() {
        std::fs::remove_file(resolved_dest).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn export_logs(app_handle: tauri::AppHandle, dest_path: String) -> Result<String, String> {
    use tauri::Manager;
    let log_dir = app_handle.path().app_log_dir().map_err(|e| e.to_string())?;
    let log_file = log_dir.join("firelink.log");
    let src = if log_file.exists() {
        log_file
    } else {
        let mut found = None;
        if let Ok(mut entries) = tokio::fs::read_dir(&log_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().is_some_and(|e| e == "log") {
                    found = Some(entry.path());
                    break;
                }
            }
        }
        found.ok_or_else(|| "No log file found in app log directory".to_string())?
    };
    tokio::fs::copy(&src, &dest_path).await.map_err(|e| e.to_string())?;
    Ok(dest_path)
}

#[tauri::command]
fn toggle_tray_icon(app_handle: tauri::AppHandle, show: bool) -> Result<(), String> {
    use tauri::tray::TrayIconBuilder;
    use tauri::menu::{Menu, MenuItem};

    if show {
        if app_handle.tray_by_id("main").is_none() {
            let quit_i = MenuItem::with_id(&app_handle, "quit", "Quit", true, None::<&str>).map_err(|e| e.to_string())?;
            let show_i = MenuItem::with_id(&app_handle, "show", "Show Firelink", true, None::<&str>).map_err(|e| e.to_string())?;
            let menu = Menu::with_items(&app_handle, &[&show_i, &quit_i]).map_err(|e| e.to_string())?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/trayTemplate.png"))
                .map_err(|e| e.to_string())?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        restore_main_window(app);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        restore_main_window(tray.app_handle());
                    }
                })
                .build(&app_handle)
                .map_err(|e| e.to_string())?;
        }
    } else {
        if let Some(_tray) = app_handle.tray_by_id("main") {
            let _ = app_handle.remove_tray_by_id("main");
        }
    }
    Ok(())
}

#[tauri::command]
fn set_extension_pairing_token(
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<(), String> {
    if token.is_empty() || token.len() > 512 {
        return Err("Invalid extension pairing token".to_string());
    }

    let mut pairing_token = state
        .extension_pairing_token
        .write()
        .map_err(|_| "Extension pairing token lock is unavailable".to_string())?;
    *pairing_token = token;
    Ok(())
}

#[tauri::command]
fn set_extension_frontend_ready(
    state: tauri::State<'_, AppState>,
    ready: bool,
) {
    state
        .extension_frontend_ready
        .store(ready, Ordering::Release);
    let coordinator = state.download_coordinator.clone();
    tauri::async_runtime::spawn(async move {
        let _ = coordinator
            .send(download::DownloadCmd::FrontendReady(ready))
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::{collect_download_uris, parse_firelink_urls};

    #[test]
    fn collects_primary_url_and_unique_mirrors_in_order() {
        let uris = collect_download_uris(
            "https://primary.example/file.zip",
            Some(
                "\nhttps://mirror-one.example/file.zip\n\
                 https://primary.example/file.zip\n\
                 https://mirror-two.example/file.zip\n",
            ),
        );

        assert_eq!(
            uris,
            vec![
                "https://primary.example/file.zip",
                "https://mirror-one.example/file.zip",
                "https://mirror-two.example/file.zip",
            ]
        );
    }

    #[test]
    fn parses_valid_firelink_download_urls() {
        let deep_link = url::Url::parse(
            "firelink://add?url=https%3A%2F%2Fexample.com%2Fone.zip%0Aftp%3A%2F%2Fexample.com%2Ftwo.zip",
        )
        .unwrap();

        assert_eq!(
            parse_firelink_urls([deep_link]),
            vec![
                "https://example.com/one.zip",
                "ftp://example.com/two.zip",
            ]
        );
    }

    #[test]
    fn rejects_unexpected_deep_links_and_nested_schemes() {
        let links = [
            url::Url::parse("firelink://open?url=https%3A%2F%2Fexample.com").unwrap(),
            url::Url::parse("firelink://add?url=file%3A%2F%2F%2Ftmp%2Fsecret").unwrap(),
            url::Url::parse("other://add?url=https%3A%2F%2Fexample.com").unwrap(),
        ];

        assert!(parse_firelink_urls(links).is_empty());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let extension_pairing_token = Arc::new(RwLock::new(String::new()));
    let server_pairing_token = extension_pairing_token.clone();
    let extension_frontend_ready = Arc::new(AtomicBool::new(false));
    let server_frontend_ready = extension_frontend_ready.clone();
    let (extension_server_shutdown_tx, extension_server_shutdown_rx) = tokio::sync::watch::channel(false);

    let aria2_port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(6800);
    let aria2_secret = uuid::Uuid::new_v4().to_string();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                window.show().unwrap();
                window.set_focus().unwrap();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .manage(Aria2DaemonGuard::new())
        .setup(move |app| {
            use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
            use tauri::menu::{Menu, MenuItem};

            let show_i = MenuItem::with_id(app, "show", "Show Firelink", true, None::<&str>).unwrap();
            let pause_all_i = MenuItem::with_id(app, "pause_all", "Pause All", true, None::<&str>).unwrap();
            let resume_all_i = MenuItem::with_id(app, "resume_all", "Resume All", true, None::<&str>).unwrap();
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app, &[&show_i, &pause_all_i, &resume_all_i, &quit_i]).unwrap();

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/trayTemplate.png")).unwrap();
            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => { restore_main_window(app); }
                    "pause_all" => { 
                        use tauri::Emitter;
                        let _ = app.emit("tray-action", "pause-all");
                    }
                    "resume_all" => { 
                        use tauri::Emitter;
                        let _ = app.emit("tray-action", "resume-all");
                    }
                    "quit" => { app.exit(0); }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        restore_main_window(tray.app_handle());
                    }
                })
                .build(app)
                .unwrap();

            
            let max_concurrent = {
                crate::settings::load_settings(app.handle())
                    .map(|settings| settings.max_concurrent_downloads)
                    .unwrap_or(crate::queue::DEFAULT_MAX_CONCURRENT)
            };

            let queue_manager = Arc::new(queue::QueueManager::new(app.handle().clone(), max_concurrent));
            let dispatcher_mgr = Arc::clone(&queue_manager);
            tauri::async_runtime::spawn(async move {
                dispatcher_mgr.run_dispatcher().await;
            });

            let queue_manager_clone = Arc::clone(&queue_manager);
            let queue_manager_poll = Arc::clone(&queue_manager);

            app.manage(AppState {
                download_coordinator: download::DownloadCoordinator::spawn(app.handle().clone()),
                extension_pairing_token,
                extension_frontend_ready,
                extension_server_shutdown: extension_server_shutdown_tx.clone(),
                aria2_port,
                aria2_secret: aria2_secret.clone(),
                media_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
                sleep_preventer: Arc::new(Mutex::new(None)),
                queue_manager,
            });

            // Backend listener: release permits + emit terminal state for
            // native (and aria2-fallback) downloads. Idempotent for Media/aria2
            // which already release via finish_runner/handle_aria2_event.
            let completion_app = app.handle().clone();
            let completion_mgr = Arc::clone(&queue_manager_clone);
            tauri::async_runtime::spawn(async move {
                use tauri::Listener;
                let rx_complete = completion_app.listen("download-complete", move |event| {
                    let raw_id = event.payload();
                    let id: String = serde_json::from_str(raw_id)
                        .unwrap_or_else(|_| raw_id.trim_matches('"').to_string());
                    let mgr = Arc::clone(&completion_mgr);
                    tauri::async_runtime::spawn(async move {
                        mgr.apply_completion(&id, crate::queue::PendingOutcome::Complete).await;
                    });
                });
                let completion_app2 = completion_app.clone();
                let completion_mgr2 = Arc::clone(&queue_manager_clone);
                let rx_failed = completion_app2.listen("download-failed", move |event| {
                    let raw_id = event.payload();
                    let id: String = serde_json::from_str(raw_id)
                        .unwrap_or_else(|_| raw_id.trim_matches('"').to_string());
                    let mgr = Arc::clone(&completion_mgr2);
                    tauri::async_runtime::spawn(async move {
                        mgr.apply_completion(&id, crate::queue::PendingOutcome::Error("download failed".to_string())).await;
                    });
                });
                // Keep the task alive; the listeners are unregistered on drop.
                std::future::pending::<()>().await;
                let _ = rx_complete;
                let _ = rx_failed;
            });

            let deep_link_app = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                dispatch_deep_links(deep_link_app.clone(), event.urls());
            });
            match app.deep_link().get_current() {
                Ok(Some(urls)) => dispatch_deep_links(app.handle().clone(), urls),
                Ok(None) => {}
                Err(error) => eprintln!("Failed to read startup deep link: {error}"),
            }
            crate::scheduler::spawn_scheduler(app.handle().clone());

            let global_speed_limit = crate::settings::load_settings(app.handle())
                .map(|settings| settings.global_speed_limit)
                .unwrap_or_default();

            match resolve_bundled_binary_path(app.handle(), "aria2c") {
                Ok(binary_path) => {
                    let mut cmd = std::process::Command::new(&binary_path);
                    cmd.arg("--enable-rpc=true")
                        .arg(format!("--rpc-listen-port={}", aria2_port))
                        .arg(format!("--rpc-secret={}", aria2_secret))
                        .arg("--rpc-listen-all=false")
                        .arg("--continue=true")
                        .arg("--retry-wait=2")
                        .arg("--allow-overwrite=false")
                        .arg("--summary-interval=1")
                        .arg("--console-log-level=warn")
                        .arg("--download-result=hide")
                        .arg("--check-certificate=true");

                    if !global_speed_limit.is_empty() {
                        cmd.arg(format!("--max-overall-download-limit={}", global_speed_limit));
                    }

                    cmd.stdout(std::process::Stdio::null());
                    cmd.stderr(std::process::Stdio::piped());

                    match cmd.spawn() {
                        Ok(mut child) => {
                            log::info!("aria2c spawned successfully on port {}", aria2_port);

                            let daemon_app = app.handle().clone();
                            if let Some(stderr) = child.stderr.take() {
                                std::thread::spawn(move || {
                                    use std::io::BufRead;
                                    let reader = std::io::BufReader::new(stderr);
                                    for line in reader.lines().map_while(Result::ok) {
                                        let trimmed = line.trim().to_string();
                                        if let Ok(mut stderr_lock) = daemon_app.state::<Aria2DaemonGuard>().last_stderr.lock() {
                                            stderr_lock.push_str(&trimmed);
                                            stderr_lock.push('\n');
                                            let excess = stderr_lock.len().saturating_sub(8192);
                                            if excess > 0 {
                                                let _ = stderr_lock.drain(..excess);
                                            }
                                        }
                                        let lower = trimmed.to_lowercase();
                                        if lower.contains("error") || lower.contains("critical") {
                                            log::error!("aria2c stderr: {}", trimmed);
                                        }
                                    }
                                });
                            }

                            let guard = app.state::<Aria2DaemonGuard>();
                            *guard.child.lock().unwrap() = Some(child);

                            let port = aria2_port;
                            let secret = aria2_secret.clone();
                            let start = std::time::Instant::now();
                            let ready = tauri::async_runtime::block_on(async {
                                let mut last_err = String::new();
                                loop {
                                    if start.elapsed() > std::time::Duration::from_secs(5) {
                                        return Err(if last_err.is_empty() {
                                            "aria2 daemon did not become ready within 5 seconds".to_string()
                                        } else {
                                            format!("aria2 did not become ready: {last_err}")
                                        });
                                    }
                                    match rpc_call(port, &secret, "aria2.getVersion", serde_json::json!([])).await {
                                        Ok(ver) => return Ok(ver),
                                        Err(e) => {
                                            last_err = e;
                                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                        }
                                    }
                                }
                            });

                            match ready {
                                Ok(ver) => {
                                    let v = ver.get("version").and_then(|v| v.as_str()).unwrap_or("unknown");
                                    log::info!("aria2 daemon ready (version {}) on port {}", v, port);
                                }
                                Err(e) => {
                                    log::error!("aria2 daemon readiness check failed: {}", e);
                                    let guard = app.state::<Aria2DaemonGuard>();
                                    *guard.startup_error.lock().unwrap() = Some(e);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to spawn aria2c: {}", e);
                            let guard = app.state::<Aria2DaemonGuard>();
                            *guard.startup_error.lock().unwrap() = Some(format!("Failed to spawn aria2c: {e}"));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to resolve aria2c binary: {}", e);
                    let guard = app.state::<Aria2DaemonGuard>();
                    *guard.startup_error.lock().unwrap() = Some(format!("Failed to resolve aria2c: {e}"));
                }
            }

            let app_handle_ws = app.handle().clone();
            let ws_port = aria2_port;
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let ws_url = format!("ws://127.0.0.1:{}/jsonrpc", ws_port);
                    if let Ok((ws_stream, _)) = tokio_tungstenite::connect_async(&ws_url).await {
                        use futures_util::StreamExt;
                        let (_, mut read) = ws_stream.split();
                        while let Some(msg) = read.next().await {
                            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                                        if let Some(params) = json.get("params").and_then(|p| p.as_array()) {
                                            if let Some(event) = params.first().and_then(|p| p.as_object()) {
                                                if let Some(gid) = event.get("gid").and_then(|g| g.as_str()) {
                                                    let state = app_handle_ws.state::<AppState>();
                                                    let outcome = match method {
                                                        "aria2.onDownloadComplete" => Some(crate::queue::PendingOutcome::Complete),
                                                        "aria2.onDownloadError" => {
                                                            let msg = event.get("error_message").and_then(|m| m.as_str()).unwrap_or("aria2 download error").to_string();
                                                            Some(crate::queue::PendingOutcome::Error(msg))
                                                        }
                                                        _ => None,
                                                    };
                                                    if let Some(outcome) = outcome {
                                                        Arc::clone(&state.queue_manager)
                                                            .handle_aria2_event(gid, outcome)
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Connection lost, loop and reconnect
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                }
            });

            let app_handle_poll = app.handle().clone();
            let poll_port = aria2_port;
            let poll_secret = aria2_secret.clone();
            let poll_mgr = Arc::clone(&queue_manager_poll);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                loop {
                    interval.tick().await;
                    let params = serde_json::json!([["gid", "status", "totalLength", "completedLength", "downloadSpeed", "errorMessage"]]);
                    if let Ok(active_list) = rpc_call(poll_port, &poll_secret, "aria2.tellActive", params).await {
                        if let Some(active_arr) = active_list.as_array() {
                            for status_info in active_arr {
                                let gid = status_info.get("gid").and_then(|s| s.as_str()).unwrap_or("");
                                let id = poll_mgr.aria2_gids.read().unwrap().get(gid).cloned();
                                if let Some(id) = id {
                                    let total = status_info.get("totalLength").and_then(|s| s.as_str()).unwrap_or("0").parse::<u64>().unwrap_or(0);
                                    let completed = status_info.get("completedLength").and_then(|s| s.as_str()).unwrap_or("0").parse::<u64>().unwrap_or(0);
                                    let speed_bytes = status_info.get("downloadSpeed").and_then(|s| s.as_str()).unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    
                                    let fraction = if total > 0 { completed as f64 / total as f64 } else { 0.0 };
                                    let speed = crate::download::format_speed(speed_bytes);
                                    let eta = if speed_bytes > 0.0 && total > completed {
                                        crate::download::format_duration((total - completed) as f64 / speed_bytes)
                                    } else {
                                        "-".to_string()
                                    };
                                    let size = if total > 0 {
                                        Some(crate::download::format_size(total as f64))
                                    } else {
                                        None
                                    };
                                    
                                    use tauri::Emitter;
                                    let _ = app_handle_poll.emit("download-progress", DownloadProgressEvent {
                                        id,
                                        fraction,
                                        speed,
                                        eta,
                                        size,
                                    });
                                }
                            }
                        }
                    }
                }
            });


            let ext_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = extension_server::start_server(
                    ext_app_handle,
                    server_pairing_token.clone(),
                    server_frontend_ready.clone(),
                    extension_server_shutdown_rx,
                ).await {
                    eprintln!("Browser extension server unavailable: {error}");
                }
            });
            Ok(())
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .max_file_size(10_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .format(move |out, message, _record| {
                    let msg = message.to_string();
                    if msg.contains("[download]") && msg.contains('%') {
                        return;
                    }
                    out.finish(format_args!("{}\n", msg));
                })
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_engine_status, test_ytdlp, test_aria2c, test_ffmpeg, test_deno, open_file, show_in_folder,
            pause_download, resume_download, fetch_metadata, fetch_media_metadata,
            update_dock_badge, set_prevent_sleep, get_free_space, perform_system_action,
            request_automation_permission, open_automation_settings,
            set_keychain_password, get_keychain_password, delete_keychain_password,
            check_file_exists, delete_file, toggle_tray_icon, set_extension_pairing_token,
            set_extension_frontend_ready, set_concurrent_limit, set_global_speed_limit, remove_download,
            enqueue_download, enqueue_many, move_in_queue, remove_from_queue, get_pending_order,
            commands::reveal_in_file_manager, commands::open_downloaded_file, commands::trash_download_assets,
            parity::get_system_proxy, parity::get_file_category, parity::check_for_updates, parity::is_supported_media, parity::get_supported_media_domains,
            parity::create_category_directories,
            export_logs
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<AppState>();
                let _ = state.extension_server_shutdown.send(true);
            }
        });
}
mod extension_server;
mod scheduler;
