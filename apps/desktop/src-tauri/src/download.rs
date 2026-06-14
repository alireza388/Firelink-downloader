use futures_util::StreamExt;
use crate::DownloadProgressEvent;
use reqwest::{
    header::{self, HeaderMap, HeaderName, HeaderValue},
    Client, StatusCode,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncWriteExt, BufWriter},
    sync::{mpsc, watch},
};
use uuid::Uuid;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
const WRITE_BUFFER_CAPACITY: usize = 256 * 1024;

#[derive(Debug)]
pub enum DownloadCmd {
    Start(DownloadPayload),
    Pause(Uuid),
    Cancel(Uuid),
    CaptureUrls(Vec<String>),
    FrontendReady(bool),
}

#[derive(Debug)]
pub struct DownloadPayload {
    pub id: Uuid,
    pub urls: Vec<String>,
    pub output_path: PathBuf,
    pub speed_limit: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub max_tries: u32,
    pub proxy: Option<String>,
}

#[derive(Clone)]
pub struct DownloadCoordinator {
    tx: mpsc::Sender<DownloadCmd>,
    media_tx: mpsc::Sender<MediaCmd>,
}

impl DownloadCoordinator {
    pub fn spawn(app_handle: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel(128);
        let (media_tx, media_rx) = mpsc::channel(32);
        tauri::async_runtime::spawn(run_coordinator(app_handle, rx, media_rx));
        Self { tx, media_tx }
    }

    pub async fn send(&self, command: DownloadCmd) -> Result<(), String> {
        self.tx
            .send(command)
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())
    }

    pub async fn register_media(&self, id: String) -> Result<watch::Receiver<bool>, String> {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.media_tx
            .send(MediaCmd::Register { id, cancel_tx })
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())?;
        Ok(cancel_rx)
    }

    pub async fn pause_media(&self, id: String) -> Result<(), String> {
        self.media_tx
            .send(MediaCmd::Pause(id))
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())
    }

    pub async fn finish_media(&self, id: String) {
        let _ = self.media_tx.send(MediaCmd::Finished(id)).await;
    }
}

enum MediaCmd {
    Register {
        id: String,
        cancel_tx: watch::Sender<bool>,
    },
    Pause(String),
    Finished(String),
}

#[derive(Debug, Clone, Copy)]
enum DownloadControl {
    Pause,
    Cancel,
    Replace,
}

struct ActiveDownload {
    generation: u64,
    control_tx: mpsc::Sender<DownloadControl>,
}

enum WorkerEvent {
    Finished {
        id: Uuid,
        generation: u64,
        outcome: DownloadOutcome,
    },
}

enum DownloadOutcome {
    Completed,
    Paused,
    Cancelled,
    Failed(String),
}

async fn run_coordinator(
    app_handle: AppHandle,
    mut command_rx: mpsc::Receiver<DownloadCmd>,
    mut media_rx: mpsc::Receiver<MediaCmd>,
) {
    let (worker_tx, mut worker_rx) = mpsc::channel(128);
    let mut active = HashMap::<Uuid, ActiveDownload>::new();
    let mut active_media = HashMap::<String, watch::Sender<bool>>::new();
    let mut pending_captured_urls = Vec::<String>::new();
    let mut frontend_ready = false;
    let mut next_generation = 0_u64;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };

                match command {
                    DownloadCmd::Start(payload) => {
                        if let Some(previous) = active.remove(&payload.id) {
                            let _ = previous.control_tx.send(DownloadControl::Replace).await;
                        }

                        next_generation = next_generation.wrapping_add(1);
                        let generation = next_generation;
                        let id = payload.id;
                        let (control_tx, control_rx) = mpsc::channel(1);
                        active.insert(id, ActiveDownload { generation, control_tx });

                        let app_handle = app_handle.clone();
                        let worker_tx = worker_tx.clone();
                        tauri::async_runtime::spawn(async move {
                            let outcome = download_file(app_handle, payload, control_rx).await;
                            let _ = worker_tx
                                .send(WorkerEvent::Finished { id, generation, outcome })
                                .await;
                        });
                    }
                    DownloadCmd::Pause(id) => {
                        if let Some(download) = active.remove(&id) {
                            let _ = download.control_tx.send(DownloadControl::Pause).await;
                        }
                    }
                    DownloadCmd::Cancel(id) => {
                        if let Some(download) = active.remove(&id) {
                            let _ = download.control_tx.send(DownloadControl::Cancel).await;
                        }
                    }
                    DownloadCmd::CaptureUrls(urls) => {
                        append_unique_urls(&mut pending_captured_urls, urls);
                        if frontend_ready && !pending_captured_urls.is_empty() {
                            let payload = pending_captured_urls.join("\n");
                            if app_handle.emit("deep-link-add-download", payload).is_ok() {
                                pending_captured_urls.clear();
                            }
                        }
                    }
                    DownloadCmd::FrontendReady(ready) => {
                        frontend_ready = ready;
                        if ready && !pending_captured_urls.is_empty() {
                            let payload = pending_captured_urls.join("\n");
                            if app_handle.emit("deep-link-add-download", payload).is_ok() {
                                pending_captured_urls.clear();
                            }
                        }
                    }
                }
            }
            event = worker_rx.recv() => {
                let Some(WorkerEvent::Finished { id, generation, outcome }) = event else {
                    continue;
                };

                let is_current = active
                    .get(&id)
                    .is_some_and(|download| download.generation == generation);
                if is_current {
                    active.remove(&id);
                }

                match (is_current, outcome) {
                    (true, DownloadOutcome::Completed) => {
                        let _ = app_handle.emit("download-complete", id.to_string());
                    }
                    (true, DownloadOutcome::Failed(error)) => {
                        eprintln!("download {id} failed: {error}");
                        let _ = app_handle.emit("download-failed", id.to_string());
                    }
                    _ => {}
                }
            }
            command = media_rx.recv() => {
                let Some(command) = command else {
                    continue;
                };
                match command {
                    MediaCmd::Register { id, cancel_tx } => {
                        if let Some(previous) = active_media.insert(id, cancel_tx) {
                            let _ = previous.send(true);
                        }
                    }
                    MediaCmd::Pause(id) => {
                        if let Some(cancel_tx) = active_media.remove(&id) {
                            let _ = cancel_tx.send(true);
                        }
                    }
                    MediaCmd::Finished(id) => {
                        active_media.remove(&id);
                    }
                }
            }
        }
    }

    for (_, download) in active {
        let _ = download.control_tx.send(DownloadControl::Cancel).await;
    }
    for (_, cancel_tx) in active_media {
        let _ = cancel_tx.send(true);
    }
}

fn append_unique_urls(target: &mut Vec<String>, urls: Vec<String>) {
    let mut seen = target.iter().cloned().collect::<HashSet<_>>();
    target.extend(urls.into_iter().filter(|url| seen.insert(url.clone())));
}

async fn download_file(
    app_handle: AppHandle,
    payload: DownloadPayload,
    mut control_rx: mpsc::Receiver<DownloadControl>,
) -> DownloadOutcome {
    if let Some(parent) = payload.output_path.parent() {
        if let Err(error) = fs::create_dir_all(parent).await {
            return DownloadOutcome::Failed(error.to_string());
        }
    }

    let client = match build_client(&payload) {
        Ok(client) => client,
        Err(error) => return DownloadOutcome::Failed(error),
    };
    let attempts_per_url = payload.max_tries.max(1);
    let mut last_error = "no download URL was provided".to_string();

    for url in &payload.urls {
        for _ in 0..attempts_per_url {
            match download_attempt(&app_handle, &client, &payload, url, &mut control_rx).await {
                Ok(()) => return DownloadOutcome::Completed,
                Err(AttemptError::Controlled(DownloadControl::Pause)) => {
                    return DownloadOutcome::Paused;
                }
                Err(AttemptError::Controlled(DownloadControl::Cancel)) => {
                    let _ = fs::remove_file(&payload.output_path).await;
                    return DownloadOutcome::Cancelled;
                }
                Err(AttemptError::Controlled(DownloadControl::Replace)) => {
                    return DownloadOutcome::Cancelled;
                }
                Err(AttemptError::Failed(error)) => last_error = error,
            }
        }
    }

    DownloadOutcome::Failed(last_error)
}

enum AttemptError {
    Controlled(DownloadControl),
    Failed(String),
}

async fn download_attempt(
    app_handle: &AppHandle,
    client: &Client,
    payload: &DownloadPayload,
    url: &str,
    control_rx: &mut mpsc::Receiver<DownloadControl>,
) -> Result<(), AttemptError> {
    let existing_len = fs::metadata(&payload.output_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut request = client.get(url);
    if existing_len > 0 {
        request = request.header(header::RANGE, format!("bytes={existing_len}-"));
    }
    if let Some(username) = payload
        .username
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        request = request.basic_auth(username, payload.password.as_deref());
    }

    let response = tokio::select! {
        control = control_rx.recv() => {
            return Err(AttemptError::Controlled(control.unwrap_or(DownloadControl::Cancel)));
        }
        response = request.send() => {
            response.map_err(|error| AttemptError::Failed(error.to_string()))?
        }
    };
    if !(response.status().is_success() || response.status() == StatusCode::PARTIAL_CONTENT) {
        return Err(AttemptError::Failed(format!(
            "{url} returned HTTP {}",
            response.status()
        )));
    }

    let resumed = existing_len > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    let completed_at_start = if resumed { existing_len } else { 0 };
    let total_len = response
        .content_length()
        .map(|remaining| remaining.saturating_add(completed_at_start));
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&payload.output_path)
        .await
        .map_err(|error| AttemptError::Failed(error.to_string()))?;
    let mut writer = BufWriter::with_capacity(WRITE_BUFFER_CAPACITY, file);
    let mut stream = response.bytes_stream();
    let mut last_emitted_at = Instant::now();
    let mut last_emitted_bytes = completed_at_start;
    let mut completed = completed_at_start;
    let speed_limit = payload.speed_limit.as_deref().and_then(parse_speed_limit);
    let transfer_started_at = Instant::now();
    let mut transferred_this_attempt = 0_u64;

    loop {
        tokio::select! {
            control = control_rx.recv() => {
                writer.flush().await.map_err(|error| AttemptError::Failed(error.to_string()))?;
                return Err(AttemptError::Controlled(control.unwrap_or(DownloadControl::Cancel)));
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        writer
                            .write_all(&bytes)
                            .await
                            .map_err(|error| AttemptError::Failed(error.to_string()))?;
                        completed = completed.saturating_add(bytes.len() as u64);
                        transferred_this_attempt =
                            transferred_this_attempt.saturating_add(bytes.len() as u64);

                        if let Some(bytes_per_second) = speed_limit {
                            let expected_elapsed =
                                Duration::from_secs_f64(transferred_this_attempt as f64 / bytes_per_second as f64);
                            let actual_elapsed = transfer_started_at.elapsed();
                            if expected_elapsed > actual_elapsed {
                                tokio::select! {
                                    control = control_rx.recv() => {
                                        writer.flush().await.map_err(|error| AttemptError::Failed(error.to_string()))?;
                                        return Err(AttemptError::Controlled(control.unwrap_or(DownloadControl::Cancel)));
                                    }
                                    _ = tokio::time::sleep(expected_elapsed - actual_elapsed) => {}
                                }
                            }
                        }

                        let now = Instant::now();
                        let interval = now.duration_since(last_emitted_at);
                        if interval >= PROGRESS_INTERVAL {
                            emit_progress(
                                app_handle,
                                payload.id,
                                completed,
                                total_len,
                                completed.saturating_sub(last_emitted_bytes),
                                interval,
                            );
                            last_emitted_at = now;
                            last_emitted_bytes = completed;
                        }
                    }
                    Some(Err(error)) => {
                        writer.flush().await.map_err(|flush_error| AttemptError::Failed(flush_error.to_string()))?;
                        return Err(AttemptError::Failed(error.to_string()));
                    }
                    None => break,
                }
            }
        }
    }

    writer
        .flush()
        .await
        .map_err(|error| AttemptError::Failed(error.to_string()))?;
    emit_progress(
        app_handle,
        payload.id,
        completed,
        total_len,
        completed.saturating_sub(last_emitted_bytes),
        last_emitted_at.elapsed(),
    );
    Ok(())
}

fn build_client(payload: &DownloadPayload) -> Result<Client, String> {
    let mut headers = HeaderMap::new();
    if let Some(raw_headers) = payload.headers.as_deref() {
        for line in raw_headers
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| format!("invalid HTTP header: {line}"))?;
            headers.insert(
                HeaderName::from_str(name.trim()).map_err(|error| error.to_string())?,
                HeaderValue::from_str(value.trim()).map_err(|error| error.to_string())?,
            );
        }
    }
    if let Some(cookies) = payload.cookies.as_deref().filter(|value| !value.is_empty()) {
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(cookies).map_err(|error| error.to_string())?,
        );
    }

    let mut builder = Client::builder().default_headers(headers);
    if let Some(user_agent) = payload
        .user_agent
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        builder = builder.user_agent(user_agent);
    }
    if let Some(proxy) = payload.proxy.as_deref().filter(|value| !value.is_empty()) {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|error| error.to_string())?);
    }

    builder.build().map_err(|error| error.to_string())
}

fn emit_progress(
    app_handle: &AppHandle,
    id: Uuid,
    completed: u64,
    total: Option<u64>,
    interval_bytes: u64,
    interval: Duration,
) {
    let speed_bytes = if interval.is_zero() {
        0.0
    } else {
        interval_bytes as f64 / interval.as_secs_f64()
    };
    let fraction = total
        .filter(|total| *total > 0)
        .map(|total| completed as f64 / total as f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let eta = total
        .filter(|total| speed_bytes > 0.0 && *total > completed)
        .map(|total| format_duration((total - completed) as f64 / speed_bytes))
        .unwrap_or_else(|| "-".to_string());

    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            id: id.to_string(),
            fraction,
            speed: format_speed(speed_bytes),
            eta,
        },
    );
}

fn format_speed(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_second / (1024.0 * 1024.0))
    } else if bytes_per_second >= 1024.0 {
        format!("{:.1} KB/s", bytes_per_second / 1024.0)
    } else {
        format!("{bytes_per_second:.0} B/s")
    }
}

fn format_duration(seconds: f64) -> String {
    if seconds >= 3600.0 {
        format!("{:.0}h {:.0}m", seconds / 3600.0, (seconds % 3600.0) / 60.0)
    } else if seconds >= 60.0 {
        format!("{:.0}m {:.0}s", seconds / 60.0, seconds % 60.0)
    } else {
        format!("{seconds:.0}s")
    }
}

fn parse_speed_limit(value: &str) -> Option<u64> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "0" {
        return None;
    }

    let (number, multiplier) = if let Some(number) = normalized.strip_suffix("kb/s") {
        (number, 1024.0)
    } else if let Some(number) = normalized.strip_suffix("mb/s") {
        (number, 1024.0 * 1024.0)
    } else if let Some(number) = normalized.strip_suffix("gb/s") {
        (number, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(number) = normalized.strip_suffix('k') {
        (number, 1024.0)
    } else if let Some(number) = normalized.strip_suffix('m') {
        (number, 1024.0 * 1024.0)
    } else if let Some(number) = normalized.strip_suffix('g') {
        (number, 1024.0 * 1024.0 * 1024.0)
    } else {
        (normalized.as_str(), 1.0)
    };

    number
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| *number > 0.0)
        .map(|number| (number * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_speed_limit;

    #[test]
    fn parses_aria_style_speed_limits() {
        assert_eq!(parse_speed_limit("512K"), Some(512 * 1024));
        assert_eq!(parse_speed_limit("1.5M"), Some(1_572_864));
        assert_eq!(parse_speed_limit("2 MB/s"), Some(2 * 1024 * 1024));
        assert_eq!(parse_speed_limit("0"), None);
    }
}
