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
async fn fetch_media_metadata(app_handle: tauri::AppHandle, url: String, cookie_browser: Option<String>, username: Option<String>, password: Option<String>) -> Result<String, String> {
    println!("fetch_media_metadata called for: {}", url);
    use tauri_plugin_shell::ShellExt;
    let mut cmd = app_handle.shell().sidecar("yt-dlp").map_err(|e| format!("Failed to create sidecar yt-dlp: {}", e))?;
    cmd = cmd.arg("-J")
       .arg("--no-warnings")
       .arg("--no-playlist")
       .arg("--no-check-formats")
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

    // We use tokio AsyncCommand so it doesn't block the async thread
    let output = cmd.output()
        .await
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(text)
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
        let first_line = text.lines().next().unwrap_or("").to_string();
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        let clean = parts.get(2).unwrap_or(&first_line.as_str()).split('-').next().unwrap_or("").to_string();
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

fn is_safe_path(path: &std::path::Path, app_handle: &tauri::AppHandle) -> bool {
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

struct Aria2DaemonGuard(std::sync::Mutex<Option<tauri_plugin_shell::process::CommandChild>>);

impl Drop for Aria2DaemonGuard {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.0.lock() {
            if let Some(child) = lock.take() {
                let _ = child.kill();
            }
        }
    }
}



pub mod download;
#[allow(dead_code)]
mod ipc;
mod parity;
pub mod error;
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
    pub aria2_port: u16,
    pub aria2_secret: String,
    pub media_semaphore: Arc<tokio::sync::Semaphore>,
    pub sleep_preventer: Arc<Mutex<Option<keepawake::KeepAwake>>>,
    pub aria2_gids: Arc<RwLock<std::collections::HashMap<String, String>>>,
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


fn resolve_path(path: &str, app_handle: &tauri::AppHandle) -> std::path::PathBuf {
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

fn collect_download_uris(url: &str, mirrors: Option<&str>) -> Vec<String> {
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

async fn rpc_call(port: u16, secret: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
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

    let client = reqwest::Client::new();
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
async fn test_aria2c(state: tauri::State<'_, AppState>) -> Result<String, String> {
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

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn start_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    url: String,
    destination: String,
    filename: String,
    connections: Option<i32>,
    speed_limit: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Option<String>,
    checksum: Option<String>,
    cookies: Option<String>,
    mirrors: Option<String>,
    user_agent: Option<String>,
    max_tries: Option<i32>,
    proxy: Option<String>,
) -> Result<(), AppError> {
    println!("start_download called for id: {}", id);
    let download_id = Uuid::parse_str(&id).map_err(|error| AppError::Internal(error.to_string()))?;

    let safe_filename = std::path::Path::new(&filename.replace('\\', "/"))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download")
        .to_string();

    let resolved_dest = resolve_path(&destination, &app_handle);
    if !is_safe_path(&resolved_dest, &app_handle) {
        return Err(AppError::Internal("Path traversal blocked".to_string()));
    }

    let mt = max_tries.unwrap_or(1).max(1) as u32;

    let mut options = serde_json::Map::new();
    options.insert("dir".to_string(), serde_json::json!(resolved_dest.to_string_lossy().to_string()));
    options.insert("out".to_string(), serde_json::json!(safe_filename));
    
    let conn = connections.unwrap_or(1);
    options.insert("split".to_string(), serde_json::json!(conn.to_string()));
    options.insert("max-connection-per-server".to_string(), serde_json::json!(conn.to_string()));
    options.insert("max-tries".to_string(), serde_json::json!(mt.to_string()));

    options.insert("continue".to_string(), serde_json::json!("true"));

    if let Some(speed) = &speed_limit {
        options.insert("max-download-limit".to_string(), serde_json::json!(speed));
    }
    if let Some(user) = &username {
        options.insert("http-user".to_string(), serde_json::json!(user));
    }
    if let Some(pass) = &password {
        options.insert("http-passwd".to_string(), serde_json::json!(pass));
    }
    if let Some(chk) = &checksum {
        options.insert("checksum".to_string(), serde_json::json!(chk));
    }
    if let Some(ua) = &user_agent {
        options.insert("user-agent".to_string(), serde_json::json!(ua));
    }

    let mut header_list = Vec::new();
    if let Some(cook) = &cookies {
        header_list.push(format!("Cookie: {}", cook));
    }
    if let Some(hdrs) = &headers {
        for line in hdrs.lines() {
            if !line.trim().is_empty() {
                header_list.push(line.trim().to_string());
            }
        }
    }
    if !header_list.is_empty() {
        options.insert("header".to_string(), serde_json::json!(header_list));
    }

    if let Some(prox) = &proxy {
        options.insert("all-proxy".to_string(), serde_json::json!(prox));
    }

    let uris = collect_download_uris(&url, mirrors.as_deref());
    let params = serde_json::json!([uris, options]);

    match rpc_call(state.aria2_port, &state.aria2_secret, "aria2.addUri", params).await {
        Ok(result) => {
            let gid = result.as_str().unwrap_or("").to_string();
            state.aria2_gids.write().unwrap().insert(id.clone(), gid);
            Ok(())
        }
        Err(e) => {
            eprintln!("aria2 failed, falling back to native coordinator: {}", e);
            state
                .download_coordinator
                .send(download::DownloadCmd::Start(Box::new(download::DownloadPayload {
                    id: download_id,
                    urls: collect_download_uris(&url, mirrors.as_deref()),
                    output_path: resolved_dest.join(safe_filename),
                    speed_limit,
                    username,
                    password,
                    headers,
                    cookies,
                    user_agent,
                    max_tries: mt,
                    proxy,
                })))
                .await
                .map_err(AppError::Internal)?;
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn start_media_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
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
) -> Result<(), String> {
    let media_semaphore = state.media_semaphore.clone();
    let coordinator = state.download_coordinator.clone();
    let mut cancel_rx = coordinator.register_media(id.clone()).await?;
    tauri::async_runtime::spawn(async move {
        let permit = tokio::select! {
            permit = media_semaphore.acquire() => permit,
            _ = cancel_rx.changed() => {
                coordinator.finish_media(id).await;
                return;
            }
        };

        if let Err(e) = start_media_download_internal(
            app_handle.clone(),
            &id, url,
            destination,
            filename,
            format_selector,
            cookie_source,
            speed_limit,
            username,
            password,
            headers,
            proxy,
            user_agent,
            max_tries,
            &mut cancel_rx,
        ).await {
            eprintln!("Media download {} failed: {}", id, e);
            use tauri::Emitter;
            let _ = app_handle.emit("download-failed", id.clone());
        }

        drop(permit);
        coordinator.finish_media(id).await;
    });

    Ok(())
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
    let mut cmd = app_handle.shell().sidecar("yt-dlp").map_err(|e| e.to_string())?
       .arg("--newline")
       .arg("--no-check-formats")
       .arg("--socket-timeout").arg("20")
       .arg("--retries").arg("3")
       .arg("--extractor-retries").arg("3")
       .arg("--downloader").arg("aria2c")
       .arg("--downloader-args").arg("aria2c:-c -x 16 -s 16 -k 1M")
       .arg("--concurrent-fragments").arg("4")
       .arg("--no-warnings")
       .arg("--continue")
       .arg("--compat-options").arg("no-youtube-unavailable-videos")
       .arg("-o").arg(out_path.to_string_lossy().to_string());

    if let Some(limit) = speed_limit {
        if !limit.is_empty() {
            cmd = cmd.arg("--limit-rate").arg(limit);
        }
    }

    if let Some(p) = proxy {
        if !p.is_empty() {
            cmd = cmd.arg("--proxy").arg(p);
        }
    }

    if let Some(mut cs) = cookie_source {
        if !cs.is_empty() && cs != "none" {
            if cs == "safari" { cs = "safari:".to_string() }
            cmd = cmd.arg("--cookies-from-browser").arg(cs);
        }
    }

    if let Some(ua) = user_agent {
        if !ua.is_empty() {
            cmd = cmd.arg("--user-agent").arg(ua);
        }
    }

    if let Some(tries) = max_tries {
        cmd = cmd.arg("--retries").arg(tries.to_string());
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
    if let Some(headers) = headers {
        for header in headers.lines().map(str::trim).filter(|header| !header.is_empty()) {
            config_content.push_str(&format!("--add-header\n{}\n", header));
        }
    }
    use std::io::Write;
    config_file.write_all(config_content.as_bytes()).map_err(|e| e.to_string())?;
    let config_path = config_file.into_temp_path();
    if !config_content.is_empty() {
        cmd = cmd.arg("--config-location").arg(config_path.to_string_lossy().to_string());
    }

    if let Some(format) = format_selector {
        cmd = cmd.arg("-f").arg(format);
        // If the filename implies an audio format, use it as audio output
        if safe_filename.ends_with(".mp3") {
            cmd = cmd.arg("-x").arg("--audio-format").arg("mp3");
        } else if safe_filename.ends_with(".m4a") {
            cmd = cmd.arg("-x").arg("--audio-format").arg("m4a");
        } else if safe_filename.ends_with(".opus") {
            cmd = cmd.arg("-x").arg("--audio-format").arg("opus");
        } else {
            // Otherwise attempt to merge into mp4 or mkv based on filename
            if safe_filename.ends_with(".mp4") {
                cmd = cmd.arg("--merge-output-format").arg("mp4");
            } else if safe_filename.ends_with(".webm") {
                cmd = cmd.arg("--merge-output-format").arg("webm");
            } else {
                cmd = cmd.arg("--merge-output-format").arg("mkv");
            }
        }
    }

    cmd = cmd.arg("--").arg(&url);

    let (mut rx, child) = cmd.spawn().map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;

    // yt-dlp parsing regex
    static PCT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SPD_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ETA_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let pct_re = PCT_RE.get_or_init(|| Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap());
    let spd_re = SPD_RE.get_or_init(|| Regex::new(r"at\s+([^\s]+)").unwrap());
    let eta_re = ETA_RE.get_or_init(|| Regex::new(r"ETA\s+([^\s]+)").unwrap());

    let _keep_alive = config_path;
    let mut current_track: f64 = 0.0;
    let mut last_fraction: f64 = 0.0;
    let mut last_progress_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(200))
        .unwrap_or_else(std::time::Instant::now);

    loop {
        tokio::select! {
            _ = cancel_rx.changed() => {
                let _ = child.kill();
                return Ok(());
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
                    Some(tauri_plugin_shell::process::CommandEvent::Stderr(_line_bytes)) => {
                        // Consume stderr to avoid blocking
                    }
                    Some(tauri_plugin_shell::process::CommandEvent::Error(err)) => {
                        eprintln!("yt-dlp shell error: {}", err);
                        let _ = app_handle.emit("download-failed", id.to_string());
                        break;
                    }
                    Some(tauri_plugin_shell::process::CommandEvent::Terminated(payload)) => {
                        println!("child exit status: {:?}", payload.code);
                        if payload.code == Some(0) {
                            let _ = app_handle.emit("download-complete", id.to_string());
                            use tauri_plugin_notification::NotificationExt;
                            let _ = app_handle.notification().builder().title("Download Complete").body(&safe_filename).show();
                        } else {
                            let _ = app_handle.emit("download-failed", id.to_string());
                        }
                        break;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn pause_download(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    println!("pause_download called for id: {}", id);
    
    let gid = state.aria2_gids.read().unwrap().get(&id).cloned();
    if let Some(g) = gid {
        let _ = rpc_call(state.aria2_port, &state.aria2_secret, "aria2.pause", serde_json::json!([g])).await;
    }

    if let Ok(download_id) = Uuid::parse_str(&id) {
        let _ = state
            .download_coordinator
            .send(download::DownloadCmd::Pause(download_id))
            .await;
    }
    state.download_coordinator.pause_media(id).await
}

#[tauri::command]
async fn remove_download(app_handle: tauri::AppHandle, state: tauri::State<'_, AppState>, id: String, filepath: Option<String>) -> Result<(), String> {
    println!("remove_download called for id: {}", id);

    if let Ok(download_id) = Uuid::parse_str(&id) {
        state
            .download_coordinator
            .send(download::DownloadCmd::Cancel(download_id))
            .await?;
    }
    state.download_coordinator.pause_media(id).await?;
    
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
async fn set_concurrent_limit(state: tauri::State<'_, AppState>, limit: usize) -> Result<(), String> {
    rpc_call(
        state.aria2_port,
        &state.aria2_secret,
        "aria2.changeGlobalOption",
        serde_json::json!([{"max-concurrent-downloads": limit.to_string()}])
    ).await.map(|_| ()).map_err(|e| {
        eprintln!("Failed to set concurrent limit: {}", e);
        e
    })
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

    let aria2_port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(6800);
    let aria2_secret = uuid::Uuid::new_v4().to_string();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            restore_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .manage(Aria2DaemonGuard(std::sync::Mutex::new(None)))
        .setup(move |app| {
            use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
            use tauri::menu::{Menu, MenuItem};

            let show_i = MenuItem::with_id(app, "show", "Show Firelink", true, None::<&str>).unwrap();
            let pause_all_i = MenuItem::with_id(app, "pause_all", "Pause All", true, None::<&str>).unwrap();
            let resume_all_i = MenuItem::with_id(app, "resume_all", "Resume All", true, None::<&str>).unwrap();
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app, &[&show_i, &pause_all_i, &resume_all_i, &quit_i]).unwrap();

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/trayTemplate.png")).unwrap();
            let _tray = TrayIconBuilder::with_id("main_startup")
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

            let aria2_gids = Arc::new(RwLock::new(std::collections::HashMap::new()));
            let aria2_gids_clone1 = aria2_gids.clone();
            let aria2_gids_clone2 = aria2_gids.clone();
            
            app.manage(AppState {
                download_coordinator: download::DownloadCoordinator::spawn(app.handle().clone()),
                extension_pairing_token,
                extension_frontend_ready,
                aria2_port,
                aria2_secret: aria2_secret.clone(),
                media_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
                sleep_preventer: Arc::new(Mutex::new(None)),
                aria2_gids,
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

            use tauri_plugin_shell::ShellExt;
            let aria2_process = match app.handle().shell().sidecar("aria2c") {
                Ok(cmd) => {
                    cmd.arg("--enable-rpc=true")
                        .arg(format!("--rpc-listen-port={}", aria2_port))
                        .arg(format!("--rpc-secret={}", aria2_secret))
                        .arg("--rpc-listen-all=false")
                        .arg("--continue=true")
                        .arg("--allow-overwrite=false")
                        .arg("--summary-interval=1")
                        .arg("--console-log-level=warn")
                        .arg("--download-result=hide")
                        .arg("--check-certificate=true")
                        .spawn()
                        .map(|(_, child)| child)
                        .ok()
                }
                Err(e) => {
                    eprintln!("Failed to create aria2c sidecar: {}", e);
                    None
                }
            };

            match aria2_process {
                Some(process) => {
                    println!("Spawned global aria2c daemon on port {}", aria2_port);
                    let guard = app.state::<Aria2DaemonGuard>();
                    *guard.0.lock().unwrap() = Some(process);
                }
                None => eprintln!("Failed to spawn aria2c daemon"),
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
                                                    let id = {
                                                        let map = aria2_gids_clone1.read().unwrap();
                                                        map.iter().find(|(_, g)| *g == gid).map(|(i, _)| i.clone())
                                                    };
                                                    if let Some(id) = id {
                                                        use tauri::Emitter;
                                                        match method {
                                                            "aria2.onDownloadComplete" => {
                                                                let _ = app_handle_ws.emit("download-complete", id.clone());
                                                                use tauri_plugin_notification::NotificationExt;
                                                                let _ = app_handle_ws.notification().builder().title("Download Complete").body(&format!("File downloaded successfully")).show();
                                                            }
                                                            "aria2.onDownloadError" => {
                                                                let _ = app_handle_ws.emit("download-failed", id);
                                                            }
                                                            _ => {}
                                                        }
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
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                loop {
                    interval.tick().await;
                    let params = serde_json::json!([["gid", "status", "totalLength", "completedLength", "downloadSpeed", "errorMessage"]]);
                    if let Ok(active_list) = rpc_call(poll_port, &poll_secret, "aria2.tellActive", params).await {
                        if let Some(active_arr) = active_list.as_array() {
                            for status_info in active_arr {
                                let gid = status_info.get("gid").and_then(|s| s.as_str()).unwrap_or("");
                                let id = {
                                    let map = aria2_gids_clone2.read().unwrap();
                                    map.iter().find(|(_, g)| *g == gid).map(|(i, _)| i.clone())
                                };
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
                ).await {
                    eprintln!("Browser extension server unavailable: {error}");
                }
            });
            Ok(())
        })
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
            test_ytdlp, test_aria2c, test_ffmpeg, test_deno, open_file, show_in_folder,
            start_download, start_media_download, pause_download, fetch_metadata, fetch_media_metadata,
            update_dock_badge, set_prevent_sleep, get_free_space, perform_system_action,
            request_automation_permission, open_automation_settings,
            set_keychain_password, get_keychain_password, delete_keychain_password,
            check_file_exists, delete_file, toggle_tray_icon, set_extension_pairing_token,
            set_extension_frontend_ready, set_concurrent_limit, set_global_speed_limit, remove_download,
            parity::get_system_proxy, parity::get_file_category, parity::check_for_updates, parity::is_supported_media, parity::get_supported_media_domains,
            parity::create_category_directories
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
mod extension_server;
mod scheduler;
