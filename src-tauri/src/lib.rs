#![allow(unexpected_cfgs)]

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;
use ts_rs::TS;

pub(crate) fn ensure_reqwest_crypto_provider() {
    static INSTALL: OnceLock<()> = OnceLock::new();
    INSTALL.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn sanitize_metadata_filename(filename: &str) -> Option<String> {
    let normalized = filename.trim().replace('\\', "/");
    let basename = std::path::Path::new(&normalized)
        .file_name()?
        .to_str()?
        .trim()
        .trim_end_matches(['.', ' ']);

    if basename.is_empty() || basename == "." || basename == ".." || basename.len() > 255 {
        return None;
    }

    Some(basename.to_string())
}

fn percent_decode_metadata_value(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(decoded).ok()
}

fn filename_from_content_disposition(disposition: &str) -> Option<String> {
    let mut fallback = None;

    for part in disposition.split(';').map(str::trim) {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };
        let normalized_name = name.trim().to_ascii_lowercase();
        let raw_value = value.trim().trim_matches('"').trim_matches('\'');

        if normalized_name == "filename*" {
            let encoded = raw_value
                .split_once("''")
                .map(|(_, value)| value)
                .unwrap_or(raw_value);
            if let Some(filename) = percent_decode_metadata_value(encoded)
                .and_then(|value| sanitize_metadata_filename(&value))
            {
                return Some(filename);
            }
        } else if normalized_name == "filename" {
            fallback = sanitize_metadata_filename(raw_value);
        }
    }

    fallback
}

fn filename_from_url_disposition_query(raw_url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(raw_url).ok()?;

    for (name, value) in parsed.query_pairs() {
        if name.eq_ignore_ascii_case("response-content-disposition")
            || name.eq_ignore_ascii_case("rscd")
        {
            if let Some(filename) = filename_from_content_disposition(&value) {
                return Some(filename);
            }
        }
    }

    None
}

fn filename_from_url_path(raw_url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(raw_url).ok()?;
    let last = parsed.path_segments()?.next_back()?;
    sanitize_metadata_filename(last)
}

fn metadata_filename_from_response(
    response: &reqwest::Response,
    current_url: &str,
    original_url: &str,
) -> String {
    if let Some(filename) = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition)
    {
        return filename;
    }

    filename_from_url_disposition_query(current_url)
        .or_else(|| filename_from_url_path(original_url))
        .or_else(|| filename_from_url_path(current_url))
        .unwrap_or_else(|| "download".to_string())
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MetadataResponse {
    url: String,
    filename: String,
    size: String,
    #[ts(type = "number")]
    size_bytes: u64,
    resumable: bool,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct MediaFormat {
    pub format_id: String,
    pub resolution: String,
    pub ext: String,
    pub format_label: String,
    #[ts(type = "number | null")]
    pub fps: Option<f64>,
    #[ts(type = "number | null")]
    pub filesize: Option<u64>,
    #[ts(type = "number | null")]
    pub filesize_approx: Option<u64>,
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

fn media_output_template(
    resolved_dest: &std::path::Path,
    safe_filename: &str,
    format_selector: Option<&str>,
) -> std::path::PathBuf {
    if format_selector.is_none() && std::path::Path::new(safe_filename).extension().is_none() {
        resolved_dest.join("%(title).200B [%(id)s].%(ext)s")
    } else {
        resolved_dest.join(safe_filename)
    }
}

fn media_progress_args() -> Vec<String> {
    vec![
        "--newline".to_string(),
        "--progress".to_string(),
        "--progress-delta".to_string(),
        "0.2".to_string(),
        "--progress-template".to_string(),
        format!("download:{MEDIA_PROGRESS_PREFIX}%(progress)j"),
    ]
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

fn json_lower(value: &serde_json::Value, key: &str) -> String {
    json_str(value, key).unwrap_or_default().to_lowercase()
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
}

fn json_f64(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

fn media_filesize(value: &serde_json::Value) -> Option<u64> {
    json_u64(value, "filesize").or_else(|| json_u64(value, "filesize_approx"))
}

fn media_exact_filesize(value: &serde_json::Value) -> Option<u64> {
    json_u64(value, "filesize")
}

fn media_approx_filesize(value: &serde_json::Value) -> Option<u64> {
    media_exact_filesize(value)
        .is_none()
        .then(|| json_u64(value, "filesize_approx"))
        .flatten()
}

fn media_sized_bytes(value: &serde_json::Value) -> Option<(u64, bool)> {
    if let Some(size) = media_exact_filesize(value) {
        Some((size, false))
    } else {
        media_approx_filesize(value).map(|size| (size, true))
    }
}

fn estimated_stream_bytes(
    value: &serde_json::Value,
    duration_seconds: Option<f64>,
) -> Option<(u64, bool)> {
    if let Some(size) = media_sized_bytes(value) {
        return Some(size);
    }

    let duration_seconds = duration_seconds.filter(|duration| *duration > 0.0)?;
    let bitrate_kbps = if has_video_stream(value) && !has_audio_stream(value) {
        json_f64(value, "vbr").or_else(|| json_f64(value, "tbr"))
    } else if has_audio_stream(value) && !has_video_stream(value) {
        json_f64(value, "abr").or_else(|| json_f64(value, "tbr"))
    } else {
        json_f64(value, "tbr")
    }
    .filter(|bitrate| *bitrate > 0.0)?;

    let bytes = bitrate_kbps * 1_000.0 * duration_seconds / 8.0;
    (bytes.is_finite() && bytes > 0.0).then_some((bytes.round() as u64, true))
}

fn split_size_estimate(bytes: Option<(u64, bool)>) -> (Option<u64>, Option<u64>) {
    match bytes {
        Some((bytes, false)) => (Some(bytes), None),
        Some((bytes, true)) => (None, Some(bytes)),
        None => (None, None),
    }
}

fn codec_is_present(codec: Option<&str>) -> bool {
    codec
        .map(str::trim)
        .filter(|codec| !codec.is_empty())
        .map(|codec| codec.to_lowercase() != "none")
        .unwrap_or(false)
}

fn has_video_stream(value: &serde_json::Value) -> bool {
    codec_is_present(json_str(value, "vcodec"))
}

fn has_audio_stream(value: &serde_json::Value) -> bool {
    codec_is_present(json_str(value, "acodec"))
}

fn is_excluded_yt_dlp_format(value: &serde_json::Value) -> bool {
    let ext = json_lower(value, "ext");
    let protocol = json_lower(value, "protocol");
    if ext == "mhtml" || protocol.contains("mhtml") {
        return true;
    }

    for key in ["format_note", "format", "format_id", "protocol"] {
        let text = json_lower(value, key);
        if text.contains("storyboard")
            || text.contains("thumbnail")
            || text.contains("subtitle")
            || text.contains("subtitles")
        {
            return true;
        }
    }

    !(has_video_stream(value) || has_audio_stream(value))
}

fn format_height(value: &serde_json::Value) -> Option<u64> {
    if let Some(height) = json_u64(value, "height").filter(|height| *height > 0) {
        return Some(height);
    }

    if let Some(resolution) = json_str(value, "resolution") {
        if let Some((_, height)) = resolution
            .split_once('x')
            .or_else(|| resolution.split_once('X'))
        {
            if let Ok(parsed) = height.trim().parse::<u64>() {
                if parsed > 0 {
                    return Some(parsed);
                }
            }
        }
    }

    let note = json_lower(value, "format_note");
    let bytes = note.as_bytes();
    let mut heights = Vec::new();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'p' || index == 0 {
            continue;
        }
        let mut start = index;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start < index {
            if let Ok(height) = note[start..index].parse::<u64>() {
                if height > 0 {
                    heights.push(height);
                }
            }
        }
    }
    heights.into_iter().max()
}

fn matches_media_height(value: &serde_json::Value, target: u64) -> bool {
    if !has_video_stream(value) {
        return false;
    }

    format_height(value) == Some(target)
}

fn format_score(value: &serde_json::Value) -> u64 {
    let height_score = format_height(value).unwrap_or(0).saturating_mul(1_000_000);
    let bitrate_score = json_f64(value, "tbr").unwrap_or(0.0).max(0.0) as u64 * 1_000;
    let size_score = media_filesize(value).unwrap_or(0).min(999);
    height_score + bitrate_score + size_score
}

fn best_matching_format<'a, F>(
    formats: &'a [&'a serde_json::Value],
    predicate: F,
) -> Option<&'a serde_json::Value>
where
    F: Fn(&serde_json::Value) -> bool,
{
    formats
        .iter()
        .copied()
        .filter(|format| predicate(format))
        .max_by_key(|format| format_score(format))
}

fn best_audio_format<'a>(
    formats: &'a [&'a serde_json::Value],
    ext: Option<&str>,
) -> Option<&'a serde_json::Value> {
    best_matching_format(formats, |format| {
        if !has_audio_stream(format) || has_video_stream(format) {
            return false;
        }
        ext.map(|wanted| json_lower(format, "ext") == wanted)
            .unwrap_or(true)
    })
}

fn display_codec(codec: Option<&str>, fallback: &str) -> String {
    let Some(codec) = codec.map(str::trim).filter(|codec| !codec.is_empty()) else {
        return fallback.to_string();
    };

    let lower = codec.to_lowercase();
    if lower == "none" {
        return fallback.to_string();
    }

    if lower.starts_with("avc1") || lower.contains("h264") {
        "H.264".to_string()
    } else if lower.starts_with("av01") {
        "AV1".to_string()
    } else if lower.starts_with("vp09") || lower.starts_with("vp9") {
        "VP9".to_string()
    } else if lower.starts_with("vp8") {
        "VP8".to_string()
    } else if lower.starts_with("hev1")
        || lower.starts_with("hvc1")
        || lower.contains("h265")
        || lower.contains("hevc")
    {
        "H.265".to_string()
    } else if lower.starts_with("mp4a") || lower.contains("aac") {
        "AAC".to_string()
    } else if lower.contains("opus") {
        "Opus".to_string()
    } else if lower.contains("vorbis") {
        "Vorbis".to_string()
    } else if lower.contains("mp3") {
        "MP3".to_string()
    } else {
        fallback.to_string()
    }
}

fn joined_format_label(
    container: &str,
    video_codec: Option<&str>,
    audio_codec: Option<&str>,
) -> String {
    let mut codecs = Vec::new();
    if video_codec.is_some() {
        codecs.push(display_codec(video_codec, "Video"));
    }
    if audio_codec.is_some() {
        codecs.push(display_codec(audio_codec, "Audio"));
    }

    if codecs.is_empty() {
        container.to_uppercase()
    } else {
        format!("{} • {}", container.to_uppercase(), codecs.join(" + "))
    }
}

fn selected_format_id(
    video: Option<&serde_json::Value>,
    audio: Option<&serde_json::Value>,
) -> Option<String> {
    match (
        video.and_then(|format| json_str(format, "format_id")),
        audio.and_then(|format| json_str(format, "format_id")),
    ) {
        (Some(video_id), Some(audio_id)) if video_id != audio_id => {
            Some(format!("{video_id}+{audio_id}"))
        }
        (Some(video_id), _) => Some(video_id.to_string()),
        (None, Some(audio_id)) => Some(audio_id.to_string()),
        (None, None) => None,
    }
}

fn estimated_merged_size(
    video: Option<&serde_json::Value>,
    audio: Option<&serde_json::Value>,
    duration_seconds: Option<f64>,
) -> (Option<u64>, Option<u64>) {
    let video_has_audio = video.is_some_and(has_audio_stream);
    let v_bytes = video.and_then(|format| estimated_stream_bytes(format, duration_seconds));
    let a_bytes = if video_has_audio {
        None
    } else {
        audio.and_then(|format| estimated_stream_bytes(format, duration_seconds))
    };

    match (video, audio, v_bytes, a_bytes, video_has_audio) {
        (Some(_), _, Some((v_size, v_approx)), _, true) => {
            split_size_estimate(Some((v_size, v_approx)))
        }
        (Some(_), Some(_), Some((v_size, v_approx)), Some((a_size, a_approx)), false) => {
            split_size_estimate(Some((v_size.saturating_add(a_size), v_approx || a_approx)))
        }
        (Some(_), None, Some((v_size, v_approx)), None, false) => {
            split_size_estimate(Some((v_size, v_approx)))
        }
        (None, Some(_), None, Some((a_size, a_approx)), false) => {
            split_size_estimate(Some((a_size, a_approx)))
        }
        _ => (None, None),
    }
}

fn raw_media_format(
    value: &serde_json::Value,
    duration_seconds: Option<f64>,
) -> Option<MediaFormat> {
    let format_id = json_str(value, "format_id")?.to_string();
    let ext = json_str(value, "ext").unwrap_or("mkv").to_string();
    let fps = json_f64(value, "fps");
    let (filesize, filesize_approx) =
        split_size_estimate(estimated_stream_bytes(value, duration_seconds));

    let resolution = if has_video_stream(value) {
        format_height(value)
            .map(|height| format!("{height}p"))
            .or_else(|| json_str(value, "resolution").map(ToOwned::to_owned))
            .unwrap_or_else(|| "Video".to_string())
    } else {
        "Audio only".to_string()
    };

    let format_label = if has_video_stream(value) && has_audio_stream(value) {
        joined_format_label(&ext, json_str(value, "vcodec"), json_str(value, "acodec"))
    } else if has_video_stream(value) {
        joined_format_label(&ext, json_str(value, "vcodec"), None)
    } else {
        joined_format_label(&ext, None, json_str(value, "acodec"))
    };

    Some(MediaFormat {
        format_id,
        resolution,
        ext,
        format_label,
        fps,
        filesize,
        filesize_approx,
    })
}

fn build_media_format_options(
    formats_arr: &[serde_json::Value],
    duration_seconds: Option<f64>,
) -> Vec<MediaFormat> {
    let clean_formats: Vec<&serde_json::Value> = formats_arr
        .iter()
        .filter(|format| !is_excluded_yt_dlp_format(format))
        .collect();
    let has_video = clean_formats.iter().any(|format| has_video_stream(format));
    let has_audio = clean_formats.iter().any(|format| has_audio_stream(format));
    let mut options = Vec::new();

    if has_video {
        let available_heights: Vec<u64> = clean_formats
            .iter()
            .filter(|format| has_video_stream(format))
            .filter_map(|format| format_height(format))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .rev()
            .collect();

        for height in available_heights {
            if let Some(video) = best_matching_format(&clean_formats, |format| {
                has_video_stream(format) && matches_media_height(format, height)
            }) {
                let audio = if has_audio_stream(video) {
                    None
                } else {
                    best_audio_format(&clean_formats, None)
                };
                let (filesize, filesize_approx) =
                    estimated_merged_size(Some(video), audio, duration_seconds);
                options.push(MediaFormat {
                    format_id: selected_format_id(Some(video), audio).unwrap_or_else(|| {
                        format!("bestvideo[height<={height}]+bestaudio/best[height<={height}]")
                    }),
                    resolution: format!("{height}p"),
                    ext: "mkv".to_string(),
                    format_label: joined_format_label(
                        "mkv",
                        json_str(video, "vcodec"),
                        audio.and_then(|format| json_str(format, "acodec")),
                    ),
                    fps: json_f64(video, "fps"),
                    filesize,
                    filesize_approx,
                });
            }

            if let Some(video) = best_matching_format(&clean_formats, |format| {
                has_video_stream(format)
                    && matches_media_height(format, height)
                    && json_lower(format, "ext") == "mp4"
            }) {
                let audio = if has_audio_stream(video) {
                    None
                } else {
                    best_audio_format(&clean_formats, Some("m4a"))
                        .or_else(|| best_audio_format(&clean_formats, None))
                };
                let (filesize, filesize_approx) =
                    estimated_merged_size(Some(video), audio, duration_seconds);
                options.push(MediaFormat {
                    format_id: selected_format_id(Some(video), audio).unwrap_or_else(|| {
                        format!("bestvideo[height<={height}][ext=mp4]+bestaudio[ext=m4a]/best[height<={height}][ext=mp4]/bestvideo[height<={height}]+bestaudio/best[height<={height}]")
                    }),
                    resolution: format!("{height}p"),
                    ext: "mp4".to_string(),
                    format_label: joined_format_label("mp4", json_str(video, "vcodec"), audio.and_then(|format| json_str(format, "acodec"))),
                    fps: json_f64(video, "fps"),
                    filesize,
                    filesize_approx,
                });
            }

            if let Some(video) = best_matching_format(&clean_formats, |format| {
                has_video_stream(format)
                    && matches_media_height(format, height)
                    && json_lower(format, "ext") == "webm"
            }) {
                let audio = if has_audio_stream(video) {
                    None
                } else {
                    best_audio_format(&clean_formats, Some("webm"))
                        .or_else(|| best_audio_format(&clean_formats, Some("opus")))
                        .or_else(|| best_audio_format(&clean_formats, None))
                };
                let (filesize, filesize_approx) =
                    estimated_merged_size(Some(video), audio, duration_seconds);
                options.push(MediaFormat {
                    format_id: selected_format_id(Some(video), audio).unwrap_or_else(|| {
                        format!("bestvideo[height<={height}][ext=webm]+bestaudio[ext=webm]/best[height<={height}][ext=webm]/bestvideo[height<={height}]+bestaudio/best[height<={height}]")
                    }),
                    resolution: format!("{height}p"),
                    ext: "webm".to_string(),
                    format_label: joined_format_label("webm", json_str(video, "vcodec"), audio.and_then(|format| json_str(format, "acodec"))),
                    fps: json_f64(video, "fps"),
                    filesize,
                    filesize_approx,
                });
            }
        }
    }

    if has_audio {
        if let Some(audio) = best_audio_format(&clean_formats, Some("m4a")) {
            let (filesize, filesize_approx) =
                split_size_estimate(estimated_stream_bytes(audio, duration_seconds));
            options.push(MediaFormat {
                format_id: json_str(audio, "format_id")
                    .unwrap_or("bestaudio[ext=m4a]/bestaudio/best")
                    .to_string(),
                resolution: "Audio only".to_string(),
                ext: "m4a".to_string(),
                format_label: joined_format_label("m4a", None, json_str(audio, "acodec")),
                fps: None,
                filesize,
                filesize_approx,
            });
        }

        if let Some(audio) = best_audio_format(&clean_formats, Some("webm"))
            .or_else(|| best_audio_format(&clean_formats, Some("opus")))
        {
            let (filesize, filesize_approx) =
                split_size_estimate(estimated_stream_bytes(audio, duration_seconds));
            options.push(MediaFormat {
                format_id: json_str(audio, "format_id")
                    .unwrap_or("bestaudio[ext=webm]/bestaudio/best")
                    .to_string(),
                resolution: "Audio only".to_string(),
                ext: "opus".to_string(),
                format_label: joined_format_label("opus", None, json_str(audio, "acodec")),
                fps: None,
                filesize,
                filesize_approx,
            });
        }

        if let Some(audio) = best_audio_format(&clean_formats, None) {
            let (filesize, filesize_approx) =
                split_size_estimate(estimated_stream_bytes(audio, duration_seconds));
            options.push(MediaFormat {
                format_id: json_str(audio, "format_id")
                    .unwrap_or("bestaudio/best")
                    .to_string(),
                resolution: "Audio only".to_string(),
                ext: "mp3".to_string(),
                format_label: "MP3 • Best audio".to_string(),
                fps: None,
                filesize,
                filesize_approx,
            });
        }
    }

    if options.is_empty() {
        clean_formats
            .iter()
            .filter_map(|format| raw_media_format(format, duration_seconds))
            .collect()
    } else {
        options
    }
}

const MEDIA_PROGRESS_PREFIX: &str = "__FIRELINK_PROGRESS__";

#[derive(Debug, PartialEq)]
struct MediaProgress {
    fraction: f64,
    speed: String,
    eta: String,
    size: Option<String>,
    downloaded_bytes: Option<f64>,
}

fn progress_json_number(progress: &serde_json::Value, key: &str) -> Option<f64> {
    progress.get(key).and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
    })
}

fn progress_json_string(progress: &serde_json::Value, key: &str) -> Option<String> {
    progress
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "N/A")
        .map(ToOwned::to_owned)
}

fn drain_media_output_lines(buffer: &mut String, chunk: &str) -> Vec<String> {
    buffer.push_str(chunk);

    let mut lines = Vec::new();
    while let Some(index) = match (buffer.find('\n'), buffer.find('\r')) {
        (Some(line_feed), Some(carriage_return)) => Some(line_feed.min(carriage_return)),
        (Some(line_feed), None) => Some(line_feed),
        (None, Some(carriage_return)) => Some(carriage_return),
        (None, None) => None,
    } {
        let mut line: String = buffer.drain(..=index).collect();
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }

    if lines.is_empty()
        && buffer.contains(MEDIA_PROGRESS_PREFIX)
        && parse_media_progress_line(buffer).is_some()
    {
        lines.push(std::mem::take(buffer));
    }

    lines
}

fn flush_media_output_line(buffer: &mut String) -> Option<String> {
    let line = std::mem::take(buffer);
    (!line.trim().is_empty()).then_some(line)
}

fn parse_media_progress_line(line: &str) -> Option<MediaProgress> {
    if let Some(prefix_index) = line.find(MEDIA_PROGRESS_PREFIX) {
        let progress: serde_json::Value =
            serde_json::from_str(line[prefix_index + MEDIA_PROGRESS_PREFIX.len()..].trim()).ok()?;
        let downloaded = progress_json_number(&progress, "downloaded_bytes").unwrap_or(0.0);
        let total = progress_json_number(&progress, "total_bytes")
            .or_else(|| progress_json_number(&progress, "total_bytes_estimate"))
            .unwrap_or(0.0);
        let fragment_index = progress_json_number(&progress, "fragment_index");
        let fragment_count = progress_json_number(&progress, "fragment_count");
        let fraction = if let (Some(fragment_index), Some(fragment_count)) =
            (fragment_index, fragment_count)
        {
            if fragment_count > 1.0 {
                (fragment_index / fragment_count).clamp(0.0, 1.0)
            } else if total > 0.0 {
                downloaded / total
            } else {
                0.0
            }
        } else if total > 0.0 {
            downloaded / total
        } else {
            progress_json_number(&progress, "_percent")
                .or_else(|| {
                    progress_json_string(&progress, "_percent_str").and_then(|percent| {
                        percent.trim_end_matches('%').trim().parse::<f64>().ok()
                    })
                })
                .unwrap_or(0.0)
                / 100.0
        };
        let speed = progress_json_string(&progress, "_speed_str")
            .or_else(|| {
                progress_json_number(&progress, "speed")
                    .filter(|speed| *speed > 0.0)
                    .map(|speed| format!("{}/s", crate::download::format_size(speed)))
            })
            .unwrap_or_else(|| "-".to_string());
        let eta = progress_json_string(&progress, "_eta_str")
            .or_else(|| {
                progress_json_number(&progress, "eta")
                    .map(|seconds| format!("{}s", seconds.round() as u64))
            })
            .unwrap_or_else(|| "-".to_string());
        let size = progress_json_string(&progress, "_total_bytes_str")
            .or_else(|| progress_json_string(&progress, "_total_bytes_estimate_str"))
            .or_else(|| (total > 0.0).then(|| crate::download::format_size(total)));

        return Some(MediaProgress {
            fraction: fraction.clamp(0.0, 1.0),
            speed,
            eta,
            size,
            downloaded_bytes: (downloaded > 0.0).then_some(downloaded),
        });
    }

    static ARIA2_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static YTDLP_PCT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static YTDLP_SPD_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static YTDLP_ETA_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static YTDLP_SIZE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let aria2_re = ARIA2_RE.get_or_init(|| {
        Regex::new(
            r"\[#[^\s]+\s+([^/\s]+)/([^\s(]+)\((\d+(?:\.\d+)?)%\).*?\bDL:([^\s\]]+)(?:\s+ETA:([^\s\]]+))?",
        )
        .unwrap()
    });
    if let Some(captures) = aria2_re.captures(line) {
        let fraction = captures.get(3)?.as_str().parse::<f64>().ok()? / 100.0;
        let speed = captures
            .get(4)
            .map(|value| format!("{}/s", value.as_str()))
            .unwrap_or_else(|| "-".to_string());
        let eta = captures
            .get(5)
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "-".to_string());
        let size = captures.get(2).map(|value| value.as_str().to_string());
        return Some(MediaProgress {
            fraction: fraction.clamp(0.0, 1.0),
            speed,
            eta,
            size,
            downloaded_bytes: None,
        });
    }

    let percent_re =
        YTDLP_PCT_RE.get_or_init(|| Regex::new(r"\[download\]\s+~?\s*(\d+(?:\.\d+)?)%").unwrap());
    let captures = percent_re.captures(line)?;
    let fraction = captures.get(1)?.as_str().parse::<f64>().ok()? / 100.0;
    let speed_re = YTDLP_SPD_RE.get_or_init(|| Regex::new(r"\bat\s+([^\s]+)").unwrap());
    let eta_re = YTDLP_ETA_RE.get_or_init(|| Regex::new(r"\bETA\s+([^\s]+)").unwrap());
    let size_re = YTDLP_SIZE_RE.get_or_init(|| Regex::new(r"of\s+~?\s*([0-9.]+[a-zA-Z]+)").unwrap());
    let mut parsed_size_str = None;
    let mut downloaded_bytes = None;
    if let Some(captures) = size_re.captures(line) {
        if let Some(size_str) = captures.get(1) {
            parsed_size_str = Some(size_str.as_str().to_string());
            if let Some(total_bytes) = parse_human_size(size_str.as_str()) {
                downloaded_bytes = Some(total_bytes * fraction);
            }
        }
    }

    Some(MediaProgress {
        fraction: fraction.clamp(0.0, 1.0),
        speed: speed_re
            .captures(line)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "-".to_string()),
        eta: eta_re
            .captures(line)
            .and_then(|capture| capture.get(1))
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "-".to_string()),
        size: parsed_size_str,
        downloaded_bytes,
    })
}

fn parse_human_size(s: &str) -> Option<f64> {
    let s = s.trim().to_lowercase();
    let num_str = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>();
    let num = num_str.parse::<f64>().ok()?;
    if s.ends_with("kib") || s.ends_with("kb") || s.ends_with("k") {
        Some(num * 1024.0)
    } else if s.ends_with("mib") || s.ends_with("mb") || s.ends_with("m") {
        Some(num * 1024.0 * 1024.0)
    } else if s.ends_with("gib") || s.ends_with("gb") || s.ends_with("g") {
        Some(num * 1024.0 * 1024.0 * 1024.0)
    } else {
        Some(num)
    }
}

const MEDIA_PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(1000);
const MEDIA_SPEED_SAMPLE_WINDOW: Duration = Duration::from_secs(8);

#[derive(Debug, Default)]
struct MediaSpeedSampler {
    samples: VecDeque<(Instant, f64)>,
}

impl MediaSpeedSampler {
    fn reset(&mut self) {
        self.samples.clear();
    }

    fn sample(&mut self, downloaded_bytes: f64, now: Instant) -> Option<f64> {
        if self
            .samples
            .back()
            .is_some_and(|(_, last_bytes)| downloaded_bytes < *last_bytes)
        {
            self.reset();
        }

        self.samples.push_back((now, downloaded_bytes));
        while self.samples.len() > 2 {
            let Some((oldest_at, _)) = self.samples.front() else {
                break;
            };
            if now.duration_since(*oldest_at) <= MEDIA_SPEED_SAMPLE_WINDOW {
                break;
            }
            self.samples.pop_front();
        }

        let (oldest_at, oldest_bytes) = *self.samples.front()?;
        let elapsed = now.duration_since(oldest_at).as_secs_f64();
        if elapsed <= 0.25 || downloaded_bytes <= oldest_bytes {
            return None;
        }

        Some((downloaded_bytes - oldest_bytes) / elapsed)
    }
}

fn media_progress_speed(
    progress: &MediaProgress,
    now: Instant,
    speed_sampler: &mut MediaSpeedSampler,
) -> (String, String) {
    let Some(downloaded_bytes) = progress.downloaded_bytes else {
        return (progress.speed.clone(), progress.eta.clone());
    };

    if let Some(bytes_per_second) = speed_sampler.sample(downloaded_bytes, now) {
        let speed_str = crate::download::format_speed(bytes_per_second);
        let eta_str = if progress.fraction > 0.0 && progress.fraction < 1.0 {
            let total = downloaded_bytes / progress.fraction;
            let remaining = total - downloaded_bytes;
            crate::download::format_duration(remaining / bytes_per_second)
        } else {
            progress.eta.clone()
        };
        (speed_str, eta_str)
    } else {
        (progress.speed.clone(), progress.eta.clone())
    }
}

fn aggregate_media_fraction(
    total_tracks: f64,
    current_track: &mut f64,
    last_fraction: &mut f64,
    next_fraction: f64,
) -> f64 {
    let next_fraction = next_fraction.clamp(0.0, 1.0);
    let has_next_track = *current_track + 1.0 < total_tracks;
    if has_next_track && *last_fraction >= 0.95 && next_fraction <= 0.05 {
        *current_track += 1.0;
        *last_fraction = next_fraction;
    } else {
        *last_fraction = (*last_fraction).max(next_fraction);
    }

    ((*current_track + *last_fraction) / total_tracks).clamp(0.0, 1.0)
}

struct MediaProgressEmitterState {
    current_track: f64,
    last_fraction: f64,
    speed_sampler: MediaSpeedSampler,
    last_progress_at: Instant,
}

impl MediaProgressEmitterState {
    fn new() -> Self {
        Self {
            current_track: 0.0,
            last_fraction: 0.0,
            speed_sampler: MediaSpeedSampler::default(),
            last_progress_at: Instant::now()
                .checked_sub(MEDIA_PROGRESS_EMIT_INTERVAL)
                .unwrap_or_else(Instant::now),
        }
    }
}

fn emit_media_progress(
    app_handle: &tauri::AppHandle,
    id: &str,
    progress: MediaProgress,
    total_tracks: f64,
    state: &mut MediaProgressEmitterState,
) {
    let previous_track = state.current_track;
    let overall_fraction = aggregate_media_fraction(
        total_tracks,
        &mut state.current_track,
        &mut state.last_fraction,
        progress.fraction,
    );
    if state.current_track != previous_track {
        state.speed_sampler.reset();
    }
    let (speed, eta) = media_progress_speed(&progress, Instant::now(), &mut state.speed_sampler);

    let now = Instant::now();
    if now.duration_since(state.last_progress_at) >= MEDIA_PROGRESS_EMIT_INTERVAL {
        let _ = app_handle.emit(
            "download-progress",
            DownloadProgressEvent {
                id: id.to_string(),
                fraction: overall_fraction,
                speed,
                eta,
                size: progress.size,
                size_is_final: false,
            },
        );
        state.last_progress_at = now;
    }
}

async fn cleanup_media_processing_artifacts(out_path: &std::path::Path) {
    cleanup_media_artifacts(out_path, true).await;
}

async fn remove_file_best_effort_with_retry(path: &std::path::Path) {
    for attempt in 0..=5 {
        match tokio::fs::remove_file(path).await {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) if attempt == 5 => {
                log::warn!(
                    "failed to remove media artifact '{}' after retries: {}",
                    path.display(),
                    error
                );
            }
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
    }
}

async fn cleanup_media_artifacts(out_path: &std::path::Path, remove_primary: bool) {
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

    if remove_primary {
        remove_file_best_effort_with_retry(out_path).await;
    }

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
            remove_file_best_effort_with_retry(&path).await;
        }
    }
}

fn sanitize_ytdlp_config_value(value: &str) -> String {
    value.replace(['\n', '\r'], "")
}

fn append_ytdlp_config_option(config: &mut String, option: &str, value: &str) {
    let safe_value = sanitize_ytdlp_config_value(value);
    if !safe_value.is_empty() {
        config.push_str(option);
        config.push(' ');
        // yt-dlp parses one configuration line at a time. Quoting keeps
        // whitespace and cookie delimiters inside this option's single value,
        // rather than turning them into additional input URLs.
        config.push('\'');
        config.push_str(&safe_value.replace('\'', "'\\''"));
        config.push('\'');
        config.push('\n');
    }
}

fn append_ytdlp_add_header(config: &mut String, header: &str) -> Result<bool, String> {
    let safe_header = sanitize_ytdlp_config_value(header).trim().to_string();
    if safe_header.is_empty() {
        return Ok(false);
    }
    let Some((name, _)) = safe_header.split_once(':') else {
        return Err(format!("invalid HTTP header: {safe_header}"));
    };
    if name.trim().is_empty() {
        return Err(format!("invalid HTTP header: {safe_header}"));
    }
    append_ytdlp_config_option(config, "--add-header", &safe_header);
    Ok(name.trim().eq_ignore_ascii_case("cookie"))
}

fn append_ytdlp_http_headers(
    config: &mut String,
    headers: Option<&str>,
    cookies: Option<&str>,
) -> Result<(), String> {
    let mut has_cookie_header = false;
    if let Some(headers) = headers {
        for header in headers.lines() {
            has_cookie_header |= append_ytdlp_add_header(config, header)?;
        }
    }

    if !has_cookie_header {
        if let Some(cookies) = cookies {
            let safe_cookies = sanitize_ytdlp_config_value(cookies).trim().to_string();
            if !safe_cookies.is_empty() {
                append_ytdlp_add_header(config, &format!("Cookie: {safe_cookies}"))?;
            }
        }
    }

    Ok(())
}

fn is_browser_cookie_extraction_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("could not copy") && lower.contains("cookie database")
        || lower.contains("could not access browser cookie database")
        || lower.contains("failed to read browser cookie")
        || lower.contains("failed to decrypt with dpapi")
}

fn should_cleanup_media_artifacts_after_failure(
    failure_reason: &str,
    strike: usize,
    max_retries: usize,
) -> bool {
    !(crate::retry::is_transient_network_error(failure_reason) && strike < max_retries)
}

async fn validate_url_ssrf(url: &str) -> Result<Option<(String, std::net::SocketAddr)>, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "SSRF blocked: Invalid URL")?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("SSRF blocked: Only HTTP/HTTPS schemes allowed".to_string());
    }
    let host = parsed.host_str().ok_or("SSRF blocked: No host")?;
    let port = parsed.port_or_known_default().unwrap_or(80);

    let mut addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "SSRF blocked: DNS resolution failed")?;

    let addr = addrs.next().ok_or("SSRF blocked: No DNS records")?;
    let ip = addr.ip();

    if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
        return Err("SSRF blocked: Private/local IP not allowed".to_string());
    }
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            if ipv4.is_private() || ipv4.is_link_local() {
                return Err("SSRF blocked: Private/local IP not allowed".to_string());
            }
        }
        std::net::IpAddr::V6(ipv6) => {
            if (ipv6.segments()[0] & 0xfe00) == 0xfc00 {
                // ULA check
                return Err("SSRF blocked: Private/local IP not allowed".to_string());
            }
            if (ipv6.segments()[0] & 0xffc0) == 0xfe80 {
                // Link-local check
                return Err("SSRF blocked: Private/local IP not allowed".to_string());
            }
        }
    }
    Ok(Some((host.to_string(), addr)))
}

#[tauri::command]
async fn fetch_metadata(
    url: String,
    user_agent: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Option<String>,
    cookies: Option<String>,
    proxy: Option<String>,
) -> Result<MetadataResponse, String> {
    ensure_reqwest_crypto_provider();

    let mut current_url = url.clone();
    let original_host = reqwest::Url::parse(&url).ok().and_then(|u| u.host_str().map(|s| s.to_string()));
    let mut redirects = 0;
    let res;

    loop {
        if redirects >= 5 {
            return Err("Too many redirects".to_string());
        }

        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(FILE_METADATA_TIMEOUT);
        if let Some(proxy) = proxy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if proxy.eq_ignore_ascii_case("none") {
                builder = builder.no_proxy();
            } else {
                builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|e| e.to_string())?);
            }
        }

        if let Some(ref ua) = user_agent {
            let ua = ua.trim();
            if !ua.is_empty() {
                builder = builder.user_agent(ua);
            } else {
                builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36");
            }
        } else {
            builder = builder.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36");
        }

        let resolved_addr = validate_url_ssrf(&current_url).await?;

        if let Some((host, addr)) = resolved_addr {
            builder = builder.resolve(&host, addr);
        }

        let current_host = reqwest::Url::parse(&current_url).ok().and_then(|u| u.host_str().map(|s| s.to_string()));
        let mut should_send_auth = redirects == 0;
        if !should_send_auth {
            if let (Some(orig), Some(curr)) = (&original_host, &current_host) {
                if curr == orig || curr.ends_with(&format!(".{}", orig)) {
                    should_send_auth = true;
                }
            }
        }

        let mut header_map = reqwest::header::HeaderMap::new();
        if should_send_auth {
            if let Some(ref h_str) = headers {
                for line in h_str.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.trim().as_bytes()) {
                            if let Ok(value) = reqwest::header::HeaderValue::from_str(v.trim()) {
                                header_map.insert(name, value);
                            }
                        }
                    }
                }
            }
            if let Some(ref c_str) = cookies {
                if !c_str.trim().is_empty() {
                    if let Ok(value) = reqwest::header::HeaderValue::from_str(c_str.trim()) {
                        header_map.insert(reqwest::header::COOKIE, value);
                    }
                }
            }
        }
        builder = builder.default_headers(header_map);

        let client = builder.build().map_err(|e| e.to_string())?;

        let build_get_range = || {
            let mut get_req = client
                .get(&current_url)
                .header(reqwest::header::RANGE, "bytes=0-0");
            if should_send_auth {
                if let Some(ref user) = username {
                    if !user.is_empty() {
                        get_req = get_req.basic_auth(user, password.as_deref());
                    }
                }
            }
            get_req
        };

        let mut head_req = client.head(&current_url);
        if should_send_auth {
            if let Some(ref user) = username {
                if !user.is_empty() {
                    head_req = head_req.basic_auth(user, password.as_deref());
                }
            }
        }
        let mut current_res = match head_req.send().await {
            Ok(response) => response,
            Err(head_error) => build_get_range().send().await.map_err(|get_error| {
                format!(
                    "HEAD metadata request failed ({head_error}); ranged GET fallback failed ({get_error})"
                )
            })?,
        };

        let mut needs_fallback = false;
        if (!current_res.status().is_success() && !current_res.status().is_redirection())
            || (current_res.status().is_success() && current_res.headers().get(reqwest::header::CONTENT_LENGTH).is_none())
        {
            needs_fallback = true;
        }

        if needs_fallback {
            current_res = build_get_range().send().await.map_err(|e| e.to_string())?;
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

    let filename = metadata_filename_from_response(&res, &current_url, &url);
    
    let mut size_str = "Unknown".to_string();
    let mut size_bytes = 0;
    
    if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        if let Some(content_range) = res.headers().get(reqwest::header::CONTENT_RANGE) {
            if let Ok(cr_str) = content_range.to_str() {
                if let Some(idx) = cr_str.find('/') {
                    if let Ok(bytes) = cr_str[idx + 1..].parse::<u64>() {
                        size_bytes = bytes;
                    }
                }
            }
        }
    }

    if size_bytes == 0 {
        if let Some(len) = res.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(len_str) = len.to_str() {
                if let Ok(bytes) = len_str.parse::<u64>() {
                    size_bytes = bytes;
                }
            }
        }
    }

    if size_bytes > 0 {
        let bytes = size_bytes;
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

    let mut resumable = false;
    if res.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        resumable = true;
    } else if let Some(accept_ranges) = res.headers().get(reqwest::header::ACCEPT_RANGES) {
        if let Ok(accept_ranges_str) = accept_ranges.to_str() {
            if accept_ranges_str.contains("bytes") {
                resumable = true;
            }
        }
    }

    Ok(MetadataResponse {
        url: current_url,
        filename,
        size: size_str,
        size_bytes,
        resumable,
    })
}

const MEDIA_METADATA_CACHE_TTL: Duration = Duration::from_secs(60);
const MEDIA_METADATA_TIMEOUT: Duration = Duration::from_secs(55);
const MEDIA_METADATA_CACHE_MAX_ENTRIES: usize = 128;
const FILE_METADATA_TIMEOUT: Duration = Duration::from_secs(20);

static MEDIA_METADATA_CACHE: OnceLock<tokio::sync::Mutex<HashMap<u64, (Instant, MediaMetadata)>>> =
    OnceLock::new();
static MEDIA_METADATA_LOCKS: OnceLock<
    tokio::sync::Mutex<HashMap<u64, std::sync::Arc<tokio::sync::Mutex<()>>>>,
> = OnceLock::new();

#[allow(clippy::too_many_arguments)] // Hash every user-controlled yt-dlp input explicitly.
fn media_metadata_cache_key(
    url: &str,
    cookie_browser: &Option<String>,
    user_agent: &Option<String>,
    username: &Option<String>,
    password: &Option<String>,
    headers: &Option<String>,
    cookies: &Option<String>,
    proxy: &Option<String>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    cookie_browser.hash(&mut hasher);
    user_agent.hash(&mut hasher);
    username.hash(&mut hasher);
    password.hash(&mut hasher);
    headers.hash(&mut hasher);
    cookies.hash(&mut hasher);
    proxy.hash(&mut hasher);
    hasher.finish()
}

async fn release_media_metadata_lock(
    cache_key: u64,
    request_lock: &std::sync::Arc<tokio::sync::Mutex<()>>,
) {
    let locks = MEDIA_METADATA_LOCKS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
    let mut locks_guard = locks.lock().await;
    if std::sync::Arc::strong_count(request_lock) == 2
        && locks_guard
            .get(&cache_key)
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, request_lock))
    {
        locks_guard.remove(&cache_key);
    }
}

fn resolve_metadata_ytdlp_path(
    app_handle: &tauri::AppHandle,
) -> Result<(PathBuf, &'static str), String> {
    resolve_bundled_binary_path(app_handle, "yt-dlp")
        .map(|path| (path, "bundled"))
        .map_err(|e| format!("failed to find bundled yt-dlp: {e}"))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Keep the generated TypeScript IPC contract flat and stable.
async fn fetch_media_metadata(
    app_handle: tauri::AppHandle,
    url: String,
    cookie_browser: Option<String>,
    user_agent: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Option<String>,
    cookies: Option<String>,
    proxy: Option<String>,
) -> Result<MediaMetadata, String> {
    validate_url_ssrf(&url).await?;
    let cache_key = media_metadata_cache_key(
        &url,
        &cookie_browser,
        &user_agent,
        &username,
        &password,
        &headers,
        &cookies,
        &proxy,
    );

    let cache = MEDIA_METADATA_CACHE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
    let mut cache_guard = cache.lock().await;
    cache_guard.retain(|_, (cached_at, _)| cached_at.elapsed() <= MEDIA_METADATA_CACHE_TTL);
    if let Some((_, metadata)) = cache_guard.get(&cache_key).cloned() {
        return Ok(metadata);
    }
    drop(cache_guard);

    let request_lock = {
        let locks = MEDIA_METADATA_LOCKS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()));
        let mut locks = locks.lock().await;
        locks
            .entry(cache_key)
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };

    let request_guard = request_lock.lock().await;

    let mut cache_guard = cache.lock().await;
    cache_guard.retain(|_, (cached_at, _)| cached_at.elapsed() <= MEDIA_METADATA_CACHE_TTL);
    if let Some((_, metadata)) = cache_guard.get(&cache_key).cloned() {
        drop(cache_guard);
        drop(request_guard);
        release_media_metadata_lock(cache_key, &request_lock).await;
        return Ok(metadata);
    }
    drop(cache_guard);

    let result = fetch_media_metadata_uncached(
        app_handle.clone(),
        url.clone(),
        cookie_browser.clone(),
        user_agent.clone(),
        username.clone(),
        password.clone(),
        headers.clone(),
        cookies.clone(),
        proxy.clone(),
    )
    .await;

    let result = match (result, cookie_browser.as_deref()) {
        (Err(error), Some(browser))
            if !browser.trim().is_empty() && is_browser_cookie_extraction_error(&error) =>
        {
            log::warn!(
                "yt-dlp could not read browser cookies from {}; retrying media metadata without browser cookies",
                browser
            );
            fetch_media_metadata_uncached(
                app_handle, url, None, user_agent, username, password, headers, cookies, proxy,
            )
            .await
        }
        (result, _) => result,
    };

    let result = match result {
        Ok(metadata) if metadata.formats.is_empty() => {
            Err("yt-dlp returned no usable media formats for this URL".to_string())
        }
        Ok(metadata) => {
            let mut cache_guard = cache.lock().await;
            if cache_guard.len() >= MEDIA_METADATA_CACHE_MAX_ENTRIES
                && !cache_guard.contains_key(&cache_key)
            {
                if let Some(oldest_key) = cache_guard
                    .iter()
                    .min_by_key(|(_, (cached_at, _))| *cached_at)
                    .map(|(key, _)| *key)
                {
                    cache_guard.remove(&oldest_key);
                }
            }
            cache_guard.insert(cache_key, (Instant::now(), metadata.clone()));
            Ok(metadata)
        }
        Err(error) => Err(error),
    };

    drop(request_guard);
    release_media_metadata_lock(cache_key, &request_lock).await;

    result
}

#[allow(clippy::too_many_arguments)] // Mirrors the stable command contract for the fallback call.
async fn fetch_media_metadata_uncached(
    app_handle: tauri::AppHandle,
    url: String,
    cookie_browser: Option<String>,
    user_agent: Option<String>,
    username: Option<String>,
    password: Option<String>,
    headers: Option<String>,
    cookies: Option<String>,
    proxy: Option<String>,
) -> Result<MediaMetadata, String> {
    // Pass bundled tools by absolute path so extraction never depends on
    // system Python, a user-managed PATH, or auto-detection heuristics.
    let deno_path = resolve_bundled_binary_path(&app_handle, "deno")
        .map_err(|e| format!("failed to find bundled deno: {e}"))?;
    let ffmpeg_path = resolve_bundled_binary_path(&app_handle, "ffmpeg")
        .map_err(|e| format!("failed to find bundled ffmpeg: {e}"))?;
    let deno_runtime = format!("deno:{}", deno_path.to_string_lossy());
    let trusted_path = crate::platform::trusted_system_path()?;

    use tauri_plugin_shell::ShellExt;
    let (ytdlp_path, _) = resolve_metadata_ytdlp_path(&app_handle)?;
    let mut cmd = app_handle
        .shell()
        .command(ytdlp_path.to_string_lossy().to_string());
    cmd = cmd
        .env("PATH", trusted_path)
        .arg("--ffmpeg-location")
        .arg(&ffmpeg_path)
        .arg("--js-runtimes")
        .arg(&deno_runtime)
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--skip-download")
        .arg("--socket-timeout")
        .arg("20")
        .arg("--retries")
        .arg("3")
        .arg("--extractor-retries")
        .arg("3")
        .arg("--compat-options")
        .arg("no-youtube-unavailable-videos")
        .arg("--print")
        .arg("%(.{title,duration,thumbnail,formats})j");

    if let Some(browser) = cookie_browser.as_deref() {
        if !browser.is_empty() {
            cmd = cmd.arg("--cookies-from-browser").arg(browser);
        }
    }

    if let Some(proxy) = proxy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if proxy.eq_ignore_ascii_case("none") {
            cmd = cmd.arg("--proxy").arg("");
        } else {
            cmd = cmd.arg("--proxy").arg(proxy);
        }
    }

    if let Some(ua) = user_agent.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        cmd = cmd.arg("--user-agent").arg(ua);
    }

    let mut config_file = tempfile::Builder::new()
        .prefix("ytdlp-")
        .suffix(".conf")
        .tempfile()
        .map_err(|e| e.to_string())?;
    let mut config_content = String::new();
    if let Some(user) = username.as_deref() {
        if !user.is_empty() {
            append_ytdlp_config_option(&mut config_content, "--username", user);
        }
    }
    if let Some(pass) = password.as_deref() {
        if !pass.is_empty() {
            append_ytdlp_config_option(&mut config_content, "--password", pass);
        }
    }
    append_ytdlp_http_headers(&mut config_content, headers.as_deref(), cookies.as_deref())?;
    use std::io::Write;
    config_file
        .write_all(config_content.as_bytes())
        .map_err(|e| e.to_string())?;
    let config_path = config_file.into_temp_path();
    if !config_content.is_empty() {
        cmd = cmd.arg("--config-location").arg(&config_path);
    }

    cmd = cmd.arg("--").arg(&url);

    let output = tokio::time::timeout(MEDIA_METADATA_TIMEOUT, cmd.output())
        .await
        .map_err(|_| {
            format!(
                "yt-dlp timed out after {}s while fetching media metadata",
                MEDIA_METADATA_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("Failed to execute yt-dlp: {}", e))?;
    if output.status.success() {
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let title = value
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Title")
            .to_string();
        let duration_seconds = value.get("duration").and_then(|v| v.as_f64());
        let duration = duration_seconds.map(|v| v as u64);
        let thumbnail = value
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let formats = value
            .get("formats")
            .and_then(|v| v.as_array())
            .map(|formats_arr| build_media_format_options(formats_arr, duration_seconds))
            .unwrap_or_default();

        Ok(MediaMetadata {
            title,
            duration,
            thumbnail,
            formats,
        })
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if err.is_empty() {
            Err(format!(
                "yt-dlp failed while fetching media metadata (exit status: {:?})",
                output.status.code()
            ))
        } else {
            Err(format!(
                "yt-dlp failed while fetching media metadata: {}",
                err
            ))
        }
    }
}

#[tauri::command]
async fn test_ytdlp(app_handle: tauri::AppHandle) -> Result<String, String> {
    let (version, error, _) = run_sidecar_version(&app_handle, "yt-dlp", &["--version"]).await;
    match (version, error) {
        (Some(version), None) => Ok(version),
        (_, Some(error)) => Err(error),
        _ => Err("yt-dlp returned no version output".to_string()),
    }
}

#[tauri::command]
async fn test_ffmpeg(app_handle: tauri::AppHandle) -> Result<String, String> {
    let (version, error, _) = run_sidecar_version(&app_handle, "ffmpeg", &["-version"]).await;
    version
        .as_deref()
        .and_then(parse_ffmpeg_version)
        .ok_or_else(|| error.unwrap_or_else(|| "ffmpeg returned no version output".to_string()))
}

#[tauri::command]
async fn test_deno(app_handle: tauri::AppHandle) -> Result<String, String> {
    let (version, error, _) = run_sidecar_version(&app_handle, "deno", &["--version"]).await;
    if let Some(text) = version {
        let re = regex::Regex::new(r"deno\s+(\d+\.\d+\.\d+)").unwrap();
        let clean = re
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or(&text)
            .to_string();
        Ok(clean)
    } else {
        Err(error.unwrap_or_else(|| "deno returned no version output".to_string()))
    }
}

pub(crate) fn is_safe_path<R: tauri::Runtime>(path: &std::path::Path, app_handle: &tauri::AppHandle<R>) -> bool {
    if !path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return false;
    }

    let Some(canonical_path) = canonicalize_with_missing_components(path) else {
        return false;
    };

    approved_download_roots(app_handle)
        .into_iter()
        .filter_map(|root| canonicalize_with_missing_components(&root))
        .any(|root| crate::platform::path_is_within(&canonical_path, &root))
}

fn canonicalize_with_missing_components(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut existing = path;
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(existing.file_name()?.to_owned());
        existing = existing.parent()?;
    }
    let mut canonical = std::fs::canonicalize(existing).ok()?;
    for component in missing.iter().rev() {
        canonical.push(component);
    }
    Some(canonical)
}

fn approved_download_roots<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> Vec<std::path::PathBuf> {
    use tauri::Manager;

    let mut roots = Vec::new();
    for root in [
        app_handle.path().download_dir().ok(),
        app_handle.path().audio_dir().ok(),
        app_handle.path().video_dir().ok(),
        app_handle.path().picture_dir().ok(),
        app_handle.path().document_dir().ok(),
        app_handle.path().desktop_dir().ok(),
    ]
    .into_iter()
    .flatten()
    {
        push_unique_path(&mut roots, root);
    }

    if let Ok(settings) = crate::settings::load_settings(app_handle) {
        push_unique_path(
            &mut roots,
            resolve_path(&settings.base_download_folder, app_handle),
        );
        for root in settings.category_directory_overrides.values() {
            push_unique_path(&mut roots, resolve_path(root, app_handle));
        }
        for root in &settings.approved_download_roots {
            push_unique_path(&mut roots, resolve_path(root, app_handle));
        }
    }

    roots
}

#[tauri::command]
fn approve_download_root(app_handle: tauri::AppHandle, path: String) -> Result<String, String> {
    let resolved = resolve_path(path.trim(), &app_handle);
    if !resolved.is_absolute() {
        return Err("Download root must be an absolute path".to_string());
    }
    let canonical = std::fs::canonicalize(&resolved)
        .map_err(|error| format!("Failed to resolve download root: {error}"))?;
    if !canonical.is_dir() {
        return Err("Download root must be an existing directory".to_string());
    }
    let canonical_text = crate::platform::display_path(&canonical);

    crate::settings::update_settings_state(&app_handle, |state| {
        let roots = state
            .entry("approvedDownloadRoots".to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if !roots.is_array() {
            *roots = serde_json::Value::Array(Vec::new());
        }
        let values = roots
            .as_array_mut()
            .expect("approved roots must be an array");
        if !values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|root| {
                crate::platform::paths_equal(
                    std::path::Path::new(root),
                    std::path::Path::new(&canonical_text),
                )
            })
        {
            values.push(serde_json::Value::String(canonical_text.clone()));
        }
    })?;

    Ok(canonical_text)
}

fn push_unique_path(paths: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    if path.is_absolute()
        && !paths
            .iter()
            .any(|existing| crate::platform::paths_equal(existing, &path))
    {
        paths.push(path);
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

struct Aria2DaemonGuard {
    child: Mutex<Option<std::process::Child>>,
    startup_error: Mutex<Option<String>>,
    last_stderr: Mutex<String>,
    config_path: Mutex<Option<tempfile::TempPath>>,
}

impl Aria2DaemonGuard {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            startup_error: Mutex::new(None),
            last_stderr: Mutex::new(String::new()),
            config_path: Mutex::new(None),
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

pub mod commands;
pub mod download;
pub mod download_ownership;
mod engines;
pub mod error;
#[allow(dead_code)]
pub mod ipc;
mod parity;
mod platform;
pub mod queue;
pub mod process;
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
    pub extension_server_port: extension_server::SharedServerPort,
    pub extension_server_shutdown: tokio::sync::watch::Sender<bool>,
    pub aria2_port: std::sync::Arc<std::sync::atomic::AtomicU16>,
    pub aria2_secret: String,
    pub media_semaphore: Arc<tokio::sync::Semaphore>,
    pub sleep_preventer: Arc<Mutex<Option<SleepPreventer>>>,
    pub scheduler_settings: Arc<RwLock<Option<crate::ipc::PersistedSettings>>>,
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
    size_is_final: bool,
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

pub(crate) fn resolve_path<R: tauri::Runtime>(path: &str, app_handle: &tauri::AppHandle<R>) -> std::path::PathBuf {
    use tauri::Manager;
    let mut resolved = std::path::PathBuf::from(path);
    if let Some(stripped) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = app_handle.path().home_dir().ok().or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .map(std::path::PathBuf::from)
        }) {
            resolved = home.join(stripped);
        } else {
            log::warn!("Failed to resolve home directory for ~ expansion");
        }
    } else if path == "~" {
        if let Some(home) = app_handle.path().home_dir().ok().or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .map(std::path::PathBuf::from)
        }) {
            resolved = home;
        } else {
            log::warn!("Failed to resolve home directory for ~ expansion");
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum FirelinkDeepLink {
    Launch,
    Add(Vec<String>),
    Invalid,
}

fn parse_firelink_deep_link(deep_link: &url::Url) -> FirelinkDeepLink {
    if deep_link.scheme() != "firelink"
        || !deep_link.username().is_empty()
        || deep_link.password().is_some()
        || deep_link.port().is_some()
    {
        return FirelinkDeepLink::Invalid;
    }

    if deep_link.host_str() == Some("launch") {
        return if matches!(deep_link.path(), "" | "/")
            && deep_link.query().is_none()
            && deep_link.fragment().is_none()
        {
            FirelinkDeepLink::Launch
        } else {
            FirelinkDeepLink::Invalid
        };
    }

    if deep_link.host_str() != Some("add")
        || !matches!(deep_link.path(), "" | "/")
        || deep_link.fragment().is_some()
    {
        return FirelinkDeepLink::Invalid;
    }

    let Some(raw_urls) = deep_link
        .query_pairs()
        .find_map(|(key, value)| (key == "url").then(|| value.into_owned()))
    else {
        return FirelinkDeepLink::Invalid;
    };
    if raw_urls.is_empty() || raw_urls.chars().count() >= MAX_DEEP_LINK_PAYLOAD_LEN {
        return FirelinkDeepLink::Invalid;
    }

    let mut captured = Vec::new();
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
                break;
            }
        }
    }

    if captured.is_empty() {
        FirelinkDeepLink::Invalid
    } else {
        FirelinkDeepLink::Add(captured)
    }
}

fn restore_main_window(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn dispatch_deep_links(app_handle: tauri::AppHandle, deep_links: Vec<url::Url>) {
    let mut should_restore = false;
    let mut urls = Vec::new();
    for deep_link in deep_links {
        match parse_firelink_deep_link(&deep_link) {
            FirelinkDeepLink::Launch => should_restore = true,
            FirelinkDeepLink::Add(parsed) => {
                should_restore = true;
                for url in parsed {
                    if !urls.contains(&url) && urls.len() < MAX_DEEP_LINK_URLS {
                        urls.push(url);
                    }
                }
            }
            FirelinkDeepLink::Invalid => {}
        }
    }
    if !should_restore {
        return;
    }

    restore_main_window(&app_handle);
    if urls.is_empty() {
        return;
    }
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

pub(crate) async fn rpc_call(
    port: u16,
    secret: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    ensure_reqwest_crypto_provider();

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

    let res = client
        .post(&url)
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
async fn test_aria2c(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let guard = app_handle.state::<Aria2DaemonGuard>();
    let startup_err = guard
        .startup_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(err) = startup_err {
        return Err(format!("aria2 daemon unavailable: {err}"));
    }

    let result = rpc_call(
        state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
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

// ── get_engine_status: Structured engine status ──────────────

async fn run_sidecar_version(
    app_handle: &tauri::AppHandle,
    sidecar_name: &str,
    args: &[&str],
) -> (Option<String>, Option<String>, Option<String>) {
    let binary_path = match resolve_bundled_binary_path(app_handle, sidecar_name) {
        Ok(p) => p,
        Err(e) => {
            return (
                None,
                Some(format!("Missing bundled binary '{}': {}", sidecar_name, e)),
                None,
            )
        }
    };

    if let Err(error) = validate_bundled_binary(&binary_path) {
        return (None, Some(error), None);
    }

    let cache_key = version_cache_key(&binary_path, args);
    if let Ok(cache) = version_check_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let mut command = tokio::process::Command::new(&binary_path);
    crate::platform::hide_tokio_child_console(&mut command);
    if sidecar_name == "aria2c" {
        crate::engines::apply_aria2_tokio_environment(&mut command, &binary_path);
    }
    command.args(args).kill_on_drop(true);
    let output_future = command.output();
    tokio::pin!(output_future);

    const VERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
    let result = tokio::time::timeout(VERSION_TIMEOUT, &mut output_future)
        .await
        .map_err(|_| VERSION_TIMEOUT);

    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return (
                None,
                Some(format!("Failed to execute '{}': {}", sidecar_name, e)),
                None,
            )
        }
        Err(timeout) => {
            return (
                None,
                Some(format!(
                    "'{}' version check timed out after {} seconds at '{}'",
                    sidecar_name,
                    timeout.as_secs(),
                    binary_path.display()
                )),
                None,
            )
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stderr_tail = if stderr.is_empty() {
        None
    } else {
        Some(stderr.clone())
    };

    let result = if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (Some(stdout), None, stderr_tail)
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let err = if !stderr.is_empty() {
            stderr.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        } else {
            format!("Exited with code {:?}", output.status.code())
        };
        (
            if stdout.is_empty() {
                None
            } else {
                Some(stdout)
            },
            Some(err),
            stderr_tail,
        )
    };

    if result.1.is_none() {
        if let Ok(mut cache) = version_check_cache().lock() {
            cache.insert(cache_key, result.clone());
        }
    }

    result
}

type VersionCheckResult = (Option<String>, Option<String>, Option<String>);

fn version_check_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, VersionCheckResult>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, VersionCheckResult>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn version_cache_key(binary_path: &std::path::Path, args: &[&str]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let modified = std::fs::metadata(binary_path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    binary_path.hash(&mut hasher);
    modified.hash(&mut hasher);
    args.hash(&mut hasher);
    hasher.finish().to_string()
}

fn validate_bundled_binary(binary_path: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::metadata(binary_path).map_err(|error| {
        format!(
            "Missing bundled binary at '{}': {error}",
            binary_path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Bundled binary path is not a file: '{}'",
            binary_path.display()
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "Bundled binary is not executable: '{}'",
                binary_path.display()
            ));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let expected_arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        if let Ok(output) = std::process::Command::new("/usr/bin/lipo")
            .arg("-archs")
            .arg(binary_path)
            .output()
        {
            if output.status.success() {
                let archs = String::from_utf8_lossy(&output.stdout);
                if !archs.split_whitespace().any(|arch| arch == expected_arch) {
                    return Err(format!(
                        "Wrong architecture for '{}': expected {}, found {}",
                        binary_path.display(),
                        expected_arch,
                        archs.trim()
                    ));
                }
            }
        }
    }

    Ok(())
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

async fn check_aria2(app_handle: &tauri::AppHandle, port: u16, secret: &str) -> EngineStatusItem {
    let sidecar_name = "aria2c";
    let expected_sidecar = crate::platform::engine_binary_name(sidecar_name);

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved
        .as_ref()
        .ok()
        .map(|p| crate::platform::display_path(p));

    let (startup_err, daemon_stderr) = {
        let guard = app_handle.state::<Aria2DaemonGuard>();
        let se = guard
            .startup_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let stderr = guard
            .last_stderr
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        (se, stderr)
    };
    let daemon_alive = startup_err.is_none();
    let last_stderr_tail = if daemon_stderr.is_empty() {
        None
    } else {
        Some(daemon_stderr)
    };

    let (version_raw, run_error, stderr_tail) =
        run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
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
    let remediation_hint = error
        .as_ref()
        .and_then(|e| generate_remediation_hint(e, sidecar_name));

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
    let expected_sidecar = crate::platform::engine_binary_name(sidecar_name);

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved
        .as_ref()
        .ok()
        .map(|p| crate::platform::display_path(p));

    let (has_internal_dir, has_python_runtime) = if let Ok(ref path) = resolved {
        let parent = path.parent().map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            let internal =
                crate::engines::ytdlp_internal_dir(path).unwrap_or_else(|| parent.join("_internal"));
            let hi = internal.is_dir();
            let hp = hi && ytdlp_embedded_runtime_exists(&internal);
            (hi, hp)
        } else {
            (false, false)
        }
    } else {
        (false, false)
    };

    let (version_raw, run_error, stderr_tail) =
        run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
    let version = version_raw.and_then(|v| v.lines().next().map(|l| l.trim().to_string()));

    let mut error = run_error;
    let mut remediation_hint = None;

    if error.is_none() && has_internal_dir && !has_python_runtime {
        error = Some(yt_dlp_missing_runtime_message().to_string());
        remediation_hint = Some(
            "The yt-dlp distribution is missing its embedded Python runtime. Reinstall Firelink."
                .to_string(),
        );
    }

    if remediation_hint.is_none() {
        remediation_hint = error
            .as_ref()
            .and_then(|e| generate_remediation_hint(e, sidecar_name));
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
        expects_internal_dir: has_internal_dir.then_some(true),
        has_internal_dir: has_internal_dir.then_some(true),
        has_python_framework: has_internal_dir.then_some(has_python_runtime),
    }
}

fn ytdlp_embedded_runtime_exists(internal: &std::path::Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        return std::fs::read_dir(internal)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .any(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                name.starts_with("python")
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
            });
    }

    #[cfg(target_os = "linux")]
    {
        return std::fs::read_dir(internal)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .any(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                name == "Python" || name.starts_with("libpython") && name.contains(".so")
            });
    }

    #[cfg(target_os = "macos")]
    {
        return internal.join("Python.framework").is_dir() || internal.join("Python").exists();
    }

    #[allow(unreachable_code)]
    false
}

fn yt_dlp_missing_runtime_message() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "_internal/python*.dll was not found beside yt-dlp sidecar";
    }

    #[cfg(target_os = "linux")]
    {
        return "_internal/libpython*.so was not found beside yt-dlp sidecar";
    }

    #[cfg(target_os = "macos")]
    {
        return "_internal/Python.framework was not found beside yt-dlp sidecar";
    }

    #[allow(unreachable_code)]
    "_internal Python runtime was not found beside yt-dlp sidecar"
}

async fn check_ffmpeg(app_handle: &tauri::AppHandle) -> EngineStatusItem {
    let sidecar_name = "ffmpeg";
    let expected_sidecar = crate::platform::engine_binary_name(sidecar_name);

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved
        .as_ref()
        .ok()
        .map(|p| crate::platform::display_path(p));

    let (version_raw, run_error, stderr_tail) =
        run_sidecar_version(app_handle, sidecar_name, &["-version"]).await;
    let version = version_raw
        .as_ref()
        .and_then(|text| parse_ffmpeg_version(text));

    let error = run_error;
    let remediation_hint = error
        .as_ref()
        .and_then(|e| generate_remediation_hint(e, sidecar_name));

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

fn parse_ffmpeg_version(output: &str) -> Option<String> {
    let first = output.lines().next()?.trim();
    let token = first
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|window| {
            window[0]
                .eq_ignore_ascii_case("version")
                .then_some(window[1])
        })?;

    let without_url = token
        .split_once("-https://")
        .or_else(|| token.split_once("-http://"))
        .map(|(prefix, _)| prefix)
        .unwrap_or(token);

    if without_url.starts_with("N-") {
        return Some(without_url.to_string());
    }

    without_url
        .split('-')
        .next()
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
}

async fn check_deno(app_handle: &tauri::AppHandle) -> EngineStatusItem {
    let sidecar_name = "deno";
    let expected_sidecar = crate::platform::engine_binary_name(sidecar_name);

    let resolved = resolve_bundled_binary_path(app_handle, sidecar_name);
    let resolved_path = resolved
        .as_ref()
        .ok()
        .map(|p| crate::platform::display_path(p));

    let (version_raw, run_error, stderr_tail) =
        run_sidecar_version(app_handle, sidecar_name, &["--version"]).await;
    let version = version_raw
        .as_ref()
        .and_then(|text| {
            let re = regex::Regex::new(r"deno\s+(\d+\.\d+\.\d+)").ok()?;
            re.captures(text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })
        .or(version_raw);

    let error = run_error;
    let remediation_hint = error
        .as_ref()
        .and_then(|e| generate_remediation_hint(e, sidecar_name));

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
    let port = state.aria2_port.load(std::sync::atomic::Ordering::Relaxed);
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

#[tauri::command]
async fn get_aria2_engine_status(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<EngineStatusItem, String> {
    Ok(check_aria2(
        &app_handle,
        state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
        &state.aria2_secret,
    )
    .await)
}

#[tauri::command]
async fn get_ytdlp_engine_status(app_handle: tauri::AppHandle) -> Result<EngineStatusItem, String> {
    Ok(check_ytdlp(&app_handle).await)
}

#[tauri::command]
async fn get_ffmpeg_engine_status(
    app_handle: tauri::AppHandle,
) -> Result<EngineStatusItem, String> {
    Ok(check_ffmpeg(&app_handle).await)
}

#[tauri::command]
async fn get_deno_engine_status(app_handle: tauri::AppHandle) -> Result<EngineStatusItem, String> {
    Ok(check_deno(&app_handle).await)
}

fn resolve_bundled_binary_path(
    app_handle: &tauri::AppHandle,
    binary_name: &str,
) -> Result<std::path::PathBuf, String> {
    crate::engines::resolve_bundled_binary_path(app_handle, binary_name)
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
    cookies: Option<String>,
    proxy: Option<String>,
    user_agent: Option<String>,
    max_tries: Option<i32>,
    cancel_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<std::path::PathBuf, String> {
    let safe_filename = crate::download_ownership::canonical_download_filename(&filename);

    let resolved_dest = resolve_path(&destination, &app_handle);

    if !is_safe_path(&resolved_dest, &app_handle) {
        return Err("Path traversal blocked".to_string());
    }

    if !resolved_dest.exists() {
        let _ = tokio::fs::create_dir_all(&resolved_dest).await;
    }

    let out_path = resolved_dest.join(&safe_filename);
    let output_template =
        media_output_template(&resolved_dest, &safe_filename, format_selector.as_deref());

    let total_tracks: f64 = if let Some(ref format) = format_selector {
        if format.contains('+') {
            2.0
        } else {
            1.0
        }
    } else {
        1.0
    };

    use tauri_plugin_shell::ShellExt;

    let mut config_file = tempfile::Builder::new()
        .prefix("ytdlp-")
        .suffix(".conf")
        .tempfile()
        .map_err(|e| e.to_string())?;
    let mut config_content = String::new();
    if let Some(user) = username.as_deref() {
        if !user.is_empty() {
            append_ytdlp_config_option(&mut config_content, "--username", user);
        }
    }
    if let Some(pass) = password.as_deref() {
        if !pass.is_empty() {
            append_ytdlp_config_option(&mut config_content, "--password", pass);
        }
    }
    append_ytdlp_http_headers(&mut config_content, headers.as_deref(), cookies.as_deref())?;
    use std::io::Write;
    config_file
        .write_all(config_content.as_bytes())
        .map_err(|e| e.to_string())?;
    let config_path = config_file.into_temp_path();

    use crate::ipc::DownloadStateEvent;
    use crate::retry::{backoff_and_emit_cancel, is_transient_network_error, BackoffOutcome};

    const STDERR_TAIL: usize = 2048;

    let config_location = if !config_content.is_empty() {
        Some(config_path.to_string_lossy().to_string())
    } else {
        None
    };
    let _keep_alive = config_path;
    let mut progress_state = MediaProgressEmitterState::new();

    // Resolve absolute paths to bundled binaries
    let aria2c_path = resolve_bundled_binary_path(&app_handle, "aria2c")?;
    let ffmpeg_path = resolve_bundled_binary_path(&app_handle, "ffmpeg")?;
    let deno_path = resolve_bundled_binary_path(&app_handle, "deno")?;
    log::info!("Using bundled aria2c: {:?}", aria2c_path);
    log::info!("Using bundled ffmpeg: {:?}", ffmpeg_path);
    log::info!("Using bundled deno: {:?}", deno_path);

    // yt-dlp accepts an absolute path for its external downloader. Keep every
    // engine explicit so behavior never depends on PATH, symlink privileges,
    // user-installed tools, or platform-specific executable aliases.
    let trusted_path = crate::platform::trusted_system_path()?;

    let max_retries = max_tries.unwrap_or(0).max(0) as usize;
    let mut strike = 0_usize;
    let mut effective_cookie_source = cookie_source.clone();
    let mut browser_cookie_fallback_used = false;

    while strike <= max_retries {
        let mut processing_started = false;
        let ytdlp_path = resolve_bundled_binary_path(&app_handle, "yt-dlp")?;
        let mut cmd = app_handle.shell().command(&ytdlp_path);
        for arg in media_progress_args() {
            cmd = cmd.arg(arg);
        }
        cmd = cmd
            .arg("--socket-timeout")
            .arg("20")
            .arg("--retries")
            // Firelink owns the retry budget so `maxAutomaticRetries` means
            // the same thing for aria2 and media downloads. Letting yt-dlp
            // also consume that value would multiply the configured retry
            // count on every outer restart.
            .arg("0")
            .arg("--extractor-retries")
            .arg("0")
            .arg("--fragment-retries")
            .arg("0")
            .arg("--ffmpeg-location")
            .arg(&ffmpeg_path)
            .arg("--js-runtimes")
            .arg(format!("deno:{}", deno_path.to_string_lossy()))
            .arg("--concurrent-fragments")
            .arg("4")
            .arg("--no-warnings")
            .arg("--continue")
            .arg("--compat-options")
            .arg("no-youtube-unavailable-videos")
            .arg("--print")
            .arg("after_move:%(filepath)s")
            .arg("-o")
            .arg(output_template.to_string_lossy().to_string())
            .env("PATH", &trusted_path);

        if let Some(limit) = speed_limit
            .as_deref()
            .and_then(normalize_speed_limit_for_aria2)
        {
            cmd = cmd.arg("--limit-rate").arg(limit);
        }

        if let Some(p) = proxy.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if p.eq_ignore_ascii_case("none") {
                cmd = cmd.arg("--proxy").arg("");
            } else {
                cmd = cmd.arg("--proxy").arg(p);
            }
        }

        if let Some(cs) = effective_cookie_source.as_ref() {
            let mut cs = cs.clone();
            if !cs.is_empty() && cs != "none" {
                if cs == "safari" {
                    cs = "safari:".to_string()
                }
                cmd = cmd.arg("--cookies-from-browser").arg(cs);
            }
        }

        if let Some(ua) = user_agent.as_ref() {
            if !ua.is_empty() {
                cmd = cmd.arg("--user-agent").arg(ua);
            }
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
                // `--merge-output-format` only affects split video/audio
                // selections. A progressive MP4/WebM stream already includes
                // audio, so also request remuxing to keep an MKV option from
                // producing a mismatched file extension and container.
                cmd = cmd
                    .arg("--merge-output-format")
                    .arg("mkv")
                    .arg("--remux-video")
                    .arg("mkv");
            }
        }

        cmd = cmd.arg("--").arg(&url);

        let (mut rx, child) = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;
        if strike > 0 {
            // The backoff path emits `Retrying`. Restore the live transfer
            // state when the replacement process actually starts so React
            // accepts progress from this attempt instead of staying stuck.
            progress_state.speed_sampler.reset();
            let _ = app_handle.emit(
                "download-state",
                DownloadStateEvent::new(id, crate::ipc::DownloadStatus::Downloading),
            );
        }
        log::info!("yt-dlp spawned for id: {} (strike {})", id, strike);

        let mut stderr_tail = String::new();
        let mut final_output_path: Option<std::path::PathBuf> = None;
        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();
        let failure_reason = loop {
            tokio::select! {
                _ = cancel_rx.changed() => {
                    crate::process::kill_process_tree(child.pid());
                    let _ = child.kill();
                    if processing_started {
                        cleanup_media_processing_artifacts(&out_path).await;
                    }
                    return Err(crate::queue::MEDIA_RUN_CANCELLED.to_string());
                }
                event = rx.recv() => {
                    match event {
                        Some(tauri_plugin_shell::process::CommandEvent::Stdout(line_bytes)) => {
                            let chunk = String::from_utf8_lossy(&line_bytes);
                            for line in drain_media_output_lines(&mut stdout_buffer, &chunk) {
                                if let Some(progress) = parse_media_progress_line(&line) {
                                    emit_media_progress(
                                        &app_handle,
                                        id,
                                        progress,
                                        total_tracks,
                                        &mut progress_state,
                                    );
                                } else {
                                    let candidate = line.trim();
                                    if !candidate.is_empty() {
                                        let candidate_path = std::path::PathBuf::from(candidate);
                                        if candidate_path.is_absolute() {
                                            final_output_path = Some(candidate_path);
                                        }
                                    }
                                }
                            }
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Stderr(line_bytes)) => {
                            let chunk = String::from_utf8_lossy(&line_bytes);
                            stderr_tail.push_str(&chunk);
                            if stderr_tail.len() > STDERR_TAIL {
                                stderr_tail = stderr_tail.split_off(stderr_tail.len() - STDERR_TAIL);
                            }
                            for line in drain_media_output_lines(&mut stderr_buffer, &chunk) {
                                if let Some(progress) = parse_media_progress_line(&line) {
                                    emit_media_progress(
                                        &app_handle,
                                        id,
                                        progress,
                                        total_tracks,
                                        &mut progress_state,
                                    );
                                }
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
                                        size_is_final: false,
                                    });
                                }
                                let lower = line.to_lowercase();
                                if lower.contains("error") || lower.contains("critical") {
                                    log::error!("yt-dlp stderr [{}]: {}", id, line.trim());
                                }
                            }
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Error(err)) => {
                            log::error!("yt-dlp shell error [{}]: {}", id, err);
                            break err;
                        }
                        Some(tauri_plugin_shell::process::CommandEvent::Terminated(payload)) => {
                            if let Some(line) = flush_media_output_line(&mut stdout_buffer) {
                                if let Some(progress) = parse_media_progress_line(&line) {
                                    emit_media_progress(
                                        &app_handle,
                                        id,
                                        progress,
                                        total_tracks,
                                        &mut progress_state,
                                    );
                                } else {
                                    let candidate = line.trim();
                                    if !candidate.is_empty() {
                                        let candidate_path = std::path::PathBuf::from(candidate);
                                        if candidate_path.is_absolute() {
                                            final_output_path = Some(candidate_path);
                                        }
                                    }
                                }
                            }
                            if let Some(line) = flush_media_output_line(&mut stderr_buffer) {
                                if let Some(progress) = parse_media_progress_line(&line) {
                                    emit_media_progress(
                                        &app_handle,
                                        id,
                                        progress,
                                        total_tracks,
                                        &mut progress_state,
                                    );
                                }
                                if !processing_started && is_media_processing_line(&line) {
                                    let _ = app_handle.emit(
                                        "download-state",
                                        DownloadStateEvent::new(
                                            id,
                                            crate::ipc::DownloadStatus::Processing,
                                        ),
                                    );
                                }
                            }
                            if payload.code == Some(0) {
                                log::info!("yt-dlp completed successfully id: {}", id);
                                let completed_path = final_output_path
                                    .as_ref()
                                    .filter(|path| path.is_file())
                                    .cloned()
                                    .unwrap_or_else(|| out_path.clone());
                                if let Ok(metadata) = tokio::fs::metadata(&completed_path).await {
                                    if metadata.is_file() {
                                        let _ = app_handle.emit("download-progress", DownloadProgressEvent {
                                            id: id.to_string(),
                                            fraction: 1.0,
                                            speed: "-".to_string(),
                                            eta: "-".to_string(),
                                            size: Some(crate::download::format_size(metadata.len() as f64)),
                                            size_is_final: true,
                                        });
                                    }
                                }
                                return Ok(completed_path);
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
        let strikes_left = strike < max_retries;
        if should_cleanup_media_artifacts_after_failure(&failure_reason, strike, max_retries) {
            cleanup_media_artifacts(&out_path, false).await;
        }
        if !browser_cookie_fallback_used
            && effective_cookie_source
                .as_deref()
                .is_some_and(|source| !source.trim().is_empty() && source != "none")
            && is_browser_cookie_extraction_error(&failure_reason)
        {
            let source = effective_cookie_source.clone().unwrap_or_default();
            log::warn!(
                "yt-dlp could not read browser cookies from {}; retrying media download without browser cookies",
                source
            );
            effective_cookie_source = None;
            browser_cookie_fallback_used = true;
            continue;
        }
        if !(transient && strikes_left) {
            return Err(failure_reason);
        }

        let reason = failure_reason.clone();
        let outcome = backoff_and_emit_cancel(strike, reason, cancel_rx, |retry_reason| {
            let _ = app_handle.emit(
                "download-state",
                DownloadStateEvent::retrying(id, retry_reason),
            );
        })
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
    let removed_pending = state.queue_manager.remove_from_pending(&id).await;

    let gid = state.queue_manager.aria2_gid_for_download(&id);
    if let Some(gid) = gid.as_deref() {
        let status = aria2_download_status(
            state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
            &state.aria2_secret,
            gid,
        )
        .await?;
        match status.as_str() {
            "paused" => {
                state.queue_manager.next_aria2_control_epoch(&id).await;
                state.queue_manager.cancel_aria2_retries(&id).await;
                log::info!("aria2 pause [{}]: gid {} was already paused", id, gid);
            }
            "active" | "waiting" => {
                state.queue_manager.next_aria2_control_epoch(&id).await;
                state.queue_manager.cancel_aria2_retries(&id).await;
                let result = rpc_call(
                    state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
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
                let retrying = state.queue_manager.has_aria2_retry_state(&id).await;
                state.queue_manager.clear_aria2_retry_state(&id).await;
                state.queue_manager.forget_aria2_gid(&id).await;
                state.queue_manager.release_permit(&id).await;
                state.queue_manager.next_aria2_control_epoch(&id).await;
                state.queue_manager.cancel_aria2_retries(&id).await;
                if retrying && matches!(terminal, "error" | "removed") {
                    use tauri::Emitter;
                    let _ = app_handle.emit(
                        "download-state",
                        crate::ipc::DownloadStateEvent::new(
                            id,
                            crate::ipc::DownloadStatus::Paused,
                        ),
                    );
                    return Ok(());
                }
                state.queue_manager.release_registered_id(&id).await;
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

    if matches!(active_kind, Some(crate::queue::TaskKind::Aria2)) {
        state.queue_manager.cancel_aria2_retries(&id).await;
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    if matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
        state
            .download_coordinator
            .pause_media_with_ack(id.clone(), tx)
            .await?;
    } else {
        let _ = tx.send(());
    }
    rx.await
        .map_err(|_| "download worker stopped without acknowledging pause".to_string())?;

    if !matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
        state.queue_manager.release_permit(&id).await;
    }
    if removed_pending || matches!(active_kind, Some(crate::queue::TaskKind::Aria2)) {
        state.queue_manager.release_registered_id(&id).await;
    }
    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id, crate::ipc::DownloadStatus::Paused),
    );
    Ok(())
}

#[tauri::command]
async fn resume_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let Some(gid) = state.queue_manager.aria2_gid_for_download(&id) else {
        log::info!(
            "aria2 resume [{}]: no mapped gid; re-enqueue is permitted",
            id
        );
        state.queue_manager.release_registered_id(&id).await;
        return Ok(false);
    };
    let status = aria2_download_status(
        state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
        &state.aria2_secret,
        &gid,
    )
    .await?;
    match status.as_str() {
        "paused" => {
            let control_epoch = state.queue_manager.next_aria2_control_epoch(&id).await;
            state.queue_manager.allow_aria2_retries(&id).await;
            use tauri::Emitter;
            let _ = app_handle.emit(
                "download-state",
                crate::ipc::DownloadStateEvent::new(&id, crate::ipc::DownloadStatus::Queued),
            );

            let queue_manager = state.queue_manager.clone();
            let aria2_port = state.aria2_port.load(std::sync::atomic::Ordering::Relaxed);
            let aria2_secret = state.aria2_secret.clone();
            let id_clone = id.clone();
            let gid_clone = gid.clone();

            let app_handle_clone = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let acquired = queue_manager.ensure_aria2_permit(&id_clone).await;
                if !acquired {
                    return;
                }
                if queue_manager.is_aria2_retry_cancelled(&id_clone).await
                    || !queue_manager
                        .is_aria2_control_epoch_current(&id_clone, control_epoch)
                        .await
                    || queue_manager.aria2_gid_for_download(&id_clone).as_deref()
                        != Some(gid_clone.as_str())
                    || !queue_manager.is_registered(&id_clone).await
                {
                    queue_manager.release_permit(&id_clone).await;
                    return;
                }
                let _ = app_handle_clone.emit(
                    "download-state",
                    crate::ipc::DownloadStateEvent::new(
                        &id_clone,
                        crate::ipc::DownloadStatus::Downloading,
                    ),
                );
                let result = match rpc_call(
                    aria2_port,
                    &aria2_secret,
                    "aria2.unpause",
                    serde_json::json!([gid_clone]),
                )
                .await
                {
                    Ok(result) => result,
                    Err(error) => {
                        queue_manager.release_permit(&id_clone).await;
                        log::error!("failed to resume aria2 gid {}: {}", gid_clone, error);
                        let _ = app_handle_clone.emit(
                            "download-state",
                            crate::ipc::DownloadStateEvent::new(
                                &id_clone,
                                crate::ipc::DownloadStatus::Failed,
                            ),
                        );
                        return;
                    }
                };
                if let Err(error) = ensure_aria2_gid_result("unpause", &gid_clone, &result) {
                    queue_manager.release_permit(&id_clone).await;
                    log::error!("failed to resume aria2 gid {}: {}", gid_clone, error);
                    let _ = app_handle_clone.emit(
                        "download-state",
                        crate::ipc::DownloadStateEvent::new(
                            &id_clone,
                            crate::ipc::DownloadStatus::Failed,
                        ),
                    );
                    return;
                }
                if queue_manager.is_aria2_retry_cancelled(&id_clone).await
                    || !queue_manager
                        .is_aria2_control_epoch_current(&id_clone, control_epoch)
                        .await
                    || queue_manager.aria2_gid_for_download(&id_clone).as_deref()
                        != Some(gid_clone.as_str())
                {
                    let _ = rpc_call(
                        aria2_port,
                        &aria2_secret,
                        "aria2.forcePause",
                        serde_json::json!([gid_clone]),
                    )
                    .await;
                    queue_manager.release_permit(&id_clone).await;
                    return;
                }
                log::info!("aria2 resume [{}]: unpaused gid {}", id_clone, gid_clone);
            });
            return Ok(true);
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
            state.queue_manager.release_registered_id(&id).await;
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
    delete_assets: bool,
) -> Result<(), String> {
    log::info!("remove_download called for id: {}", id);
    let primary_path = crate::download_ownership::primary_path_for_id(&app_handle, &id)?;

    let active_kind = state.queue_manager.active_kind(&id).await;
    state.queue_manager.remove_from_pending(&id).await;

    state.queue_manager.next_aria2_control_epoch(&id).await;
    state.queue_manager.cancel_aria2_retries(&id).await;

    let gid = state.queue_manager.aria2_gid_for_download(&id);
    if let Some(gid) = gid.as_deref() {
        let removal_result = async {
            force_remove_aria2_gid(
                state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
                &state.aria2_secret,
                gid,
            )
            .await?;
            wait_for_aria2_stopped(
                state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
                &state.aria2_secret,
                gid,
            )
            .await
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
        let (tx, rx) = tokio::sync::oneshot::channel();
        if matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
            state
                .download_coordinator
                .pause_media_with_ack(id.clone(), tx)
                .await?;
        } else {
            let _ = tx.send(());
        }
        let _ = rx.await;

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

    let cleanup_result = async {
        if delete_assets {
            if let Some(path) = primary_path.as_deref() {
                remove_download_assets(path, &app_handle).await?;
            }
        }
        crate::download_ownership::remove(&app_handle, &id)?;
        Ok::<(), String>(())
    }
    .await;

    state.queue_manager.release_registered_id(&id).await;
    cleanup_result
}

pub(crate) async fn remove_download_assets<R: tauri::Runtime>(
    primary: &std::path::Path,
    app_handle: &tauri::AppHandle<R>,
) -> Result<(), String> {
    if !is_safe_path(primary, app_handle) {
        return Err("Download asset path is outside an allowed download location".to_string());
    }

    if primary.exists() {
        let mut retries = 5;
        loop {
            let res = if primary.is_dir() {
                tokio::fs::remove_dir_all(primary).await.map_err(|e| e.to_string())
            } else {
                if let Err(e) = trash::delete(primary) {
                    log::warn!("failed to move downloaded file to Trash, attempting hard delete: {}", e);
                    std::fs::remove_file(primary).map_err(|e| e.to_string())
                } else {
                    Ok(())
                }
            };
            match res {
                Ok(_) => break,
                Err(e) => {
                    if retries == 0 {
                        return Err(e);
                    }
                    retries -= 1;
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    }

    for suffix in [".aria2", ".part", ".ytdl"] {
        let mut candidate_os = primary.as_os_str().to_os_string();
        candidate_os.push(suffix);
        let candidate = std::path::PathBuf::from(candidate_os);
        if candidate.exists() && is_safe_path(&candidate, app_handle) {
            let mut retries = 5;
            loop {
                match tokio::fs::remove_file(&candidate).await {
                    Ok(_) => break,
                    Err(_) if retries == 0 => {
                        return Err(format!("failed to remove '{}' after retries", candidate.display()));
                    }
                    Err(_) => {
                        retries -= 1;
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }

    cleanup_media_processing_artifacts(primary).await;
    Ok(())
}

#[tauri::command]
async fn detach_download_for_reconfigure(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    log::info!("detach_download_for_reconfigure called for id: {}", id);
    let active_kind = state.queue_manager.active_kind(&id).await;
    state.queue_manager.remove_from_pending(&id).await;
    state.queue_manager.next_aria2_control_epoch(&id).await;
    state.queue_manager.cancel_aria2_retries(&id).await;

    let gid = state.queue_manager.aria2_gid_for_download(&id);

    if let Some(gid) = gid.as_deref() {
        let removal_result = async {
            let pause_res = rpc_call(
                state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
                &state.aria2_secret,
                "aria2.forcePause",
                serde_json::json!([gid]),
            )
            .await;

            if let Err(e) = pause_res {
                if !e.contains("cannot be paused now") {
                    return Err(e);
                }
            }

            wait_for_aria2_stopped(
                state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
                &state.aria2_secret,
                gid,
            )
            .await
        }
        .await;
        if let Err(error) = removal_result {
            state.queue_manager.allow_aria2_retries(&id).await;
            return Err(error);
        }
        state.queue_manager.clear_aria2_retry_state(&id).await;
        state.queue_manager.forget_aria2_gid(&id).await;
        state.queue_manager.release_permit(&id).await;
        state.queue_manager.release_registered_id(&id).await;
        log::info!("aria2 detach [{}]: gid {} stopped and forgotten", id, gid);
    } else {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
            state
                .download_coordinator
                .pause_media_with_ack(id.clone(), tx)
                .await?;
        } else {
            let _ = tx.send(()); // Fallback if no task exists
        }
        let _ = rx.await; // Wait for the writer to stop

        if !matches!(active_kind, Some(crate::queue::TaskKind::Media)) {
            state.queue_manager.release_permit(&id).await;
        }
        state.queue_manager.clear_aria2_retry_state(&id).await;
        state.queue_manager.forget_aria2_gid(&id).await;
        state.queue_manager.release_registered_id(&id).await;
    }

    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id.clone(), crate::ipc::DownloadStatus::Paused),
    );

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
    match rpc_call(port, secret, "aria2.forceRemove", serde_json::json!([gid])).await {
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
            Ok(status) if matches!(status.as_str(), "paused" | "complete" | "error" | "removed") => {
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
#[allow(unused_variables)]
fn update_dock_badge(app_handle: tauri::AppHandle, count: i32) {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};
        use std::ffi::CString;

        let _ = app_handle.run_on_main_thread(move || {
            unsafe {
            let app_class = class!(NSApplication);
            let app: *mut Object = msg_send![app_class, sharedApplication];
            let dock_tile: *mut Object = msg_send![app, dockTile];
            let label = if count > 0 {
                count.to_string()
            } else {
                "".to_string()
            };
            let c_label = CString::new(label).unwrap();
            let ns_string_class = class!(NSString);
            let ns_label: *mut Object = msg_send![ns_string_class, alloc];
            let ns_label: *mut Object = msg_send![ns_label, initWithUTF8String: c_label.as_ptr()];
            let _: () = msg_send![dock_tile, setBadgeLabel: ns_label];
            let _: () = msg_send![ns_label, release];
            }
        });
    }
}

#[tauri::command]
fn get_platform_info() -> crate::ipc::PlatformInfo {
    crate::ipc::PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        target_triple: crate::platform::target_triple(),
    }
}

#[cfg(target_os = "macos")]
mod macos_sleep {
    use std::ffi::c_void;
    #[allow(clippy::duplicated_attributes)]
    #[link(name = "IOKit", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn IOPMAssertionCreateWithDescription(
            AssertionType: *const c_void,
            Name: *const c_void,
            Details: *const c_void,
            HumanReadableReason: *const c_void,
            LocalizationBundlePath: *const c_void,
            Timeout: f64,
            TimeoutAction: *const c_void,
            AssertionID: *mut u32,
        ) -> i32;
        pub fn IOPMAssertionRelease(AssertionID: u32) -> i32;
        pub fn CFStringCreateWithCString(
            alloc: *const c_void,
            cStr: *const i8,
            encoding: u32,
        ) -> *const c_void;
        pub fn CFRelease(arg: *const c_void);
    }
}

pub enum SleepPreventer {
    #[cfg(target_os = "macos")]
    Mac { system_sleep_id: u32, network_client_id: u32 },
    #[cfg(not(target_os = "macos"))]
    Other(keepawake::KeepAwake),
}

impl Drop for SleepPreventer {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let SleepPreventer::Mac { system_sleep_id, network_client_id } = self;
            unsafe {
                macos_sleep::IOPMAssertionRelease(*system_sleep_id);
                macos_sleep::IOPMAssertionRelease(*network_client_id);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn create_sleep_preventer() -> Result<SleepPreventer, String> {
    use std::ffi::CString;
    use std::ptr::null;
    unsafe {
        let create_cf_string = |s: &str| -> *const std::ffi::c_void {
            let cstr = CString::new(s).unwrap();
            macos_sleep::CFStringCreateWithCString(null(), cstr.as_ptr(), 0x08000100)
        };

        let type_sys = create_cf_string("PreventSystemSleep");
        let type_net = create_cf_string("NetworkClientActive");
        let name = create_cf_string("Firelink active download");

        let mut sys_id: u32 = 0;
        let mut net_id: u32 = 0;

        let res1 = macos_sleep::IOPMAssertionCreateWithDescription(
            type_sys, name, null(), null(), null(), 0.0, null(), &mut sys_id
        );
        let res2 = macos_sleep::IOPMAssertionCreateWithDescription(
            type_net, name, null(), null(), null(), 0.0, null(), &mut net_id
        );

        macos_sleep::CFRelease(type_sys);
        macos_sleep::CFRelease(type_net);
        macos_sleep::CFRelease(name);

        if res1 == 0 && res2 == 0 {
            Ok(SleepPreventer::Mac { system_sleep_id: sys_id, network_client_id: net_id })
        } else {
            if res1 == 0 { macos_sleep::IOPMAssertionRelease(sys_id); }
            if res2 == 0 { macos_sleep::IOPMAssertionRelease(net_id); }
            Err("Failed to create macOS sleep assertions".to_string())
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn create_sleep_preventer() -> Result<SleepPreventer, String> {
    keepawake::Builder::default()
        .idle(true)
        .reason("Firelink active download")
        .create()
        .map(SleepPreventer::Other)
        .map_err(|error| format!("failed to prevent system sleep: {error}"))
}

#[tauri::command]
fn set_prevent_sleep(state: tauri::State<'_, AppState>, prevent: bool) -> Result<(), String> {
    let mut current_preventer = state
        .sleep_preventer
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if prevent {
        if current_preventer.is_none() {
            *current_preventer = Some(create_sleep_preventer()?);
        }
    } else {
        *current_preventer = None;
    }
    Ok(())
}

pub(crate) fn execute_system_action(action: crate::ipc::PostQueueAction) -> Result<(), String> {
    match action {
        crate::ipc::PostQueueAction::Shutdown => {
            system_shutdown::shutdown().map_err(|e| e.to_string())
        }
        crate::ipc::PostQueueAction::Restart => {
            system_shutdown::reboot().map_err(|e| e.to_string())
        }
        crate::ipc::PostQueueAction::Sleep => system_shutdown::sleep().map_err(|e| e.to_string()),
        crate::ipc::PostQueueAction::None => Err("Invalid action".to_string()),
    }
}

#[tauri::command]
fn perform_system_action(action: crate::ipc::PostQueueAction) -> Result<(), String> {
    execute_system_action(action)
}

#[tauri::command]
fn ack_schedule_trigger(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    action: String,
    key: String,
) -> Result<(), String> {
    crate::settings::update_settings_state(&app_handle, |state| match action.as_str() {
        "start" => {
            state.insert("schedulerLastStartKey".to_string(), serde_json::json!(key));
        }
        "stop" => {
            state.insert("schedulerLastStopKey".to_string(), serde_json::json!(key));
        }
        _ => {}
    })?;
    match action.as_str() {
        "start" | "stop" => {
            if let Ok(mut cached) = state.scheduler_settings.write() {
                if let Some(settings) = cached.as_mut() {
                    if action == "start" {
                        settings.scheduler_last_start_key = key;
                    } else {
                        settings.scheduler_last_stop_key = key;
                    }
                }
            }
            Ok(())
        }
        _ => Err("Unknown scheduler trigger action".to_string()),
    }
}

#[tauri::command]
async fn get_pending_order(
    state: tauri::State<'_, AppState>,
    queue_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    Ok(state.queue_manager.pending_order(queue_id.as_deref()).await)
}

fn enqueue_lifecycle_generation(item: &queue::EnqueueItem) -> Result<u64, String> {
    item.lifecycle_generation
        .as_deref()
        .map(|generation| {
            generation
                .parse::<u64>()
                .map_err(|_| "Invalid enqueue lifecycle generation".to_string())
        })
        .transpose()
        .map(|generation| generation.unwrap_or_default())
}

#[tauri::command]
async fn enqueue_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mut item: queue::EnqueueItem,
) -> Result<crate::ipc::EnqueueAccepted, AppError> {
    let id = item.id.clone();
    item.filename = crate::download_ownership::canonical_download_filename(&item.filename);
    let accepted_filename = item.filename.clone();
    let lifecycle_generation = enqueue_lifecycle_generation(&item).map_err(AppError::Internal)?;
    let previous_generation = state
        .queue_manager
        .reserve_enqueue_generation(&id, lifecycle_generation)
        .await
        .map_err(AppError::Internal)?;
    if let Err(error) = crate::download_ownership::register_expected(
        &app_handle,
        &item.id,
        &item.destination,
        &item.filename,
    ) {
        state
            .queue_manager
            .rollback_enqueue_reservation(&id, lifecycle_generation, previous_generation)
            .await;
        return Err(AppError::Internal(error));
    }
    if let Err(error) = state
        .queue_manager
        .commit_reserved_enqueue(item.into_task(), lifecycle_generation)
        .await
    {
        let _ = crate::download_ownership::remove(&app_handle, &id);
        state
            .queue_manager
            .rollback_enqueue_reservation(&id, lifecycle_generation, previous_generation)
            .await;
        return Err(AppError::Internal(error));
    }
    Ok(crate::ipc::EnqueueAccepted {
        id,
        filename: accepted_filename,
    })
}

#[tauri::command]
async fn cancel_enqueue_generation(
    state: tauri::State<'_, AppState>,
    id: String,
    generation: String,
) -> Result<(), AppError> {
    let generation = generation
        .parse::<u64>()
        .map_err(|_| AppError::Internal("Invalid enqueue lifecycle generation".to_string()))?;
    state
        .queue_manager
        .cancel_enqueue_generation(&id, generation)
        .await;
    Ok(())
}

#[tauri::command]
async fn enqueue_many(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    items: Vec<queue::EnqueueItem>,
) -> Result<Vec<crate::ipc::EnqueueResult>, AppError> {
    let mut results = Vec::with_capacity(items.len());
    for mut item in items {
        item.filename = crate::download_ownership::canonical_download_filename(&item.filename);
        let id = item.id.clone();
        let filename = item.filename.clone();
        let lifecycle_generation = match enqueue_lifecycle_generation(&item) {
            Ok(generation) => generation,
            Err(error) => {
                results.push(crate::ipc::EnqueueResult {
                    id,
                    success: false,
                    filename: None,
                    error: Some(error),
                });
                continue;
            }
        };
        let previous_generation = match state
            .queue_manager
            .reserve_enqueue_generation(&id, lifecycle_generation)
            .await
        {
            Ok(previous) => previous,
            Err(error) => {
                results.push(crate::ipc::EnqueueResult {
                    id,
                    success: false,
                    filename: None,
                    error: Some(error),
                });
                continue;
            }
        };
        if let Err(error) = crate::download_ownership::register_expected(
            &app_handle,
            &item.id,
            &item.destination,
            &item.filename,
        ) {
            state
                .queue_manager
                .rollback_enqueue_reservation(&id, lifecycle_generation, previous_generation)
                .await;
            results.push(crate::ipc::EnqueueResult {
                id,
                success: false,
                filename: None,
                error: Some(error),
            });
            continue;
        }
        if let Err(error) = state
            .queue_manager
            .commit_reserved_enqueue(item.into_task(), lifecycle_generation)
            .await
        {
            let _ = crate::download_ownership::remove(&app_handle, &id);
            state
                .queue_manager
                .rollback_enqueue_reservation(&id, lifecycle_generation, previous_generation)
                .await;
            results.push(crate::ipc::EnqueueResult {
                id,
                success: false,
                filename: None,
                error: Some(error),
            });
            continue;
        }
        results.push(crate::ipc::EnqueueResult {
            id,
            success: true,
            filename: Some(filename),
            error: None,
        });
    }

    Ok(results)
}

#[tauri::command]
async fn move_in_queue(
    state: tauri::State<'_, AppState>,
    id: String,
    queue_id: String,
    direction: crate::ipc::QueueDirection,
) -> Result<Vec<String>, AppError> {
    Ok(state
        .queue_manager
        .move_in_queue(&id, &queue_id, direction)
        .await)
}

#[tauri::command]
async fn remove_from_queue(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<bool, AppError> {
    let removed = state.queue_manager.remove_from_pending(&id).await;
    if removed {
        let _ = crate::download_ownership::remove(&app_handle, &id);
        state.queue_manager.release_registered_id(&id).await;
    }
    Ok(removed)
}

#[tauri::command]
async fn set_concurrent_limit(
    state: tauri::State<'_, AppState>,
    limit: usize,
) -> Result<(), String> {
    state.queue_manager.set_capacity(limit);
    Ok(())
}

pub(crate) fn normalize_speed_limit_for_aria2(limit: &str) -> Option<String> {
    let trimmed = limit.trim();
    if trimmed.is_empty() {
        return None;
    }

    let re = regex::Regex::new(r"(?i)^(\d+(?:\.\d+)?)\s*([kmgt]?)i?b?(?:/s)?$").ok()?;
    let captures = re.captures(trimmed)?;
    let amount = captures.get(1)?.as_str().parse::<f64>().ok()?;
    if !amount.is_finite() || amount <= 0.0 {
        return None;
    }

    let unit = captures
        .get(2)
        .map(|m| m.as_str().to_ascii_uppercase())
        .unwrap_or_default();
    Some(if unit.is_empty() {
        format!("{amount}K")
    } else {
        format!("{amount}{unit}")
    })
}

#[tauri::command]
async fn set_global_speed_limit(
    state: tauri::State<'_, AppState>,
    limit: Option<String>,
) -> Result<(), String> {
    let limit_str = limit
        .as_deref()
        .and_then(normalize_speed_limit_for_aria2)
        .unwrap_or_else(|| "0".to_string());
    rpc_call(
        state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
        &state.aria2_secret,
        "aria2.changeGlobalOption",
        serde_json::json!([{"max-overall-download-limit": limit_str}]),
    )
    .await
    .map(|_| ())
    .map_err(|e| {
        eprintln!("Failed to set global speed limit: {}", e);
        e
    })
}

#[tauri::command]
fn check_automation_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};
        use std::ffi::CString;
        use std::ptr::null_mut;

        unsafe {
            objc::rc::autoreleasepool(|| {
                let script = "tell application \"System Events\" to get name";
                let c_script = CString::new(script).unwrap();
                let ns_string_class = class!(NSString);
                let script_str: *mut Object = msg_send![ns_string_class, alloc];
                let script_str: *mut Object = msg_send![script_str, initWithUTF8String: c_script.as_ptr()];

                let ns_apple_script: *mut Object = msg_send![class!(NSAppleScript), alloc];
                let ns_apple_script: *mut Object = msg_send![ns_apple_script, initWithSource: script_str];

                let mut error_dict: *mut Object = null_mut();
                let result: *mut Object = msg_send![ns_apple_script, executeAndReturnError: &mut error_dict];

                let _: () = msg_send![script_str, release];
                let _: () = msg_send![ns_apple_script, release];

                if result.is_null() {
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
fn request_automation_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        system_shutdown::request_permission_dialog()
            .map_err(|error| format!("Automation permission was not granted: {error}"))
    }

    #[cfg(not(target_os = "macos"))]
    Ok(())
}

#[tauri::command]
#[allow(unused_variables)]
fn open_automation_settings(app_handle: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        app_handle
            .opener()
            .open_url(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
                None::<String>,
            )
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
        if crate::platform::path_is_within(&resolved_dest, mount_point) {
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
    crate::db::set_keychain_password(&id, &password)
}

#[tauri::command]
fn get_keychain_password(id: String) -> Result<String, String> {
    crate::db::get_keychain_password(&id)
}

#[tauri::command]
fn delete_keychain_password(id: String) -> Result<(), String> {
    crate::db::delete_keychain_password(&id)
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
struct PairingTokenHydration {
    token: String,
    token_changed: bool,
    persistent: bool,
    error: Option<String>,
}

/// Hydrate the extension pairing token on startup **without touching the OS
/// keychain**.  The token is read from the persisted settings (SQLite) so the
/// operating system never presents a credential-access prompt before the UI is
/// visible — even after a build update where the code signature changed.
///
/// When no token has been persisted yet (fresh install) a new one is generated
/// and `persistent` is returned as `false`, which causes the frontend to show
/// the `KeychainPermissionModal`.
#[tauri::command]
fn hydrate_extension_pairing_token(
    database: tauri::State<'_, crate::db::DbState>,
    app_state: tauri::State<'_, AppState>,
) -> Result<PairingTokenHydration, String> {
    let connection = database.lock()?;

    // Primary path: read the token from the settings DB.  This is always safe
    // and never triggers an OS prompt.
    if let Some(existing) = crate::db::load_pairing_token_from_settings(&connection)? {
        if let Ok(mut pairing_token) = app_state.extension_pairing_token.write() {
            *pairing_token = existing.clone();
        }
        return Ok(PairingTokenHydration {
            token: existing,
            token_changed: false,
            persistent: true,
            error: None,
        });
    }

    // No token in the DB yet — generate one and save it so future launches
    // find it without prompting.
    let generated = crate::db::generate_pairing_token();
    crate::db::save_pairing_token_to_settings(&connection, &generated)?;
    if let Ok(mut pairing_token) = app_state.extension_pairing_token.write() {
        *pairing_token = generated.clone();
    }
    Ok(PairingTokenHydration {
        token: generated,
        token_changed: false,
        persistent: false,
        error: None,
    })
}

#[tauri::command]
fn grant_keychain_access(
    database: tauri::State<'_, crate::db::DbState>,
    app_state: tauri::State<'_, AppState>,
) -> Result<PairingTokenHydration, String> {
    let mut connection = database.lock()?;

    // Explicitly force migration of any legacy token to the keychain.
    // This is the ONLY code path that touches the OS keychain and it is
    // reached exclusively through the frontend's "Grant Access" button,
    // so any system prompt is user-initiated.
    let _ = crate::db::sanitize_current_settings_and_restore_token(&connection, true);

    match crate::db::hydrate_pairing_token(&mut connection, false) {
        Ok((token, token_changed)) => {
            // Persist the token to the settings DB so future startups
            // can read it without touching the keychain at all.
            let _ = crate::db::save_pairing_token_to_settings(&connection, &token);
            if let Ok(mut pairing_token) = app_state.extension_pairing_token.write() {
                *pairing_token = token.clone();
            }
            Ok(PairingTokenHydration {
                token,
                token_changed,
                persistent: true,
                error: None,
            })
        }
        Err(error) => {
            let token = app_state
                .extension_pairing_token
                .read()
                .map_err(|_| "Extension pairing token lock is unavailable".to_string())?
                .clone();
            Ok(PairingTokenHydration {
                token,
                token_changed: false,
                persistent: false,
                error: Some(error),
            })
        }
    }
}

#[tauri::command]
fn acknowledge_pairing_token_change(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<(), String> {
    let connection = state.lock()?;
    crate::db::acknowledge_pairing_token_notice(&connection)
}

#[tauri::command]
fn db_save_settings(
    state: tauri::State<'_, crate::db::DbState>,
    app_state: tauri::State<'_, AppState>,
    data: String,
) -> Result<(), String> {
    let connection = state.lock()?;
    let existing = crate::db::load_settings(&connection)?;
    let merged = crate::settings::preserve_scheduler_runtime_keys(existing.as_deref(), &data)?;
    crate::db::save_settings(&connection, &merged)?;
    let decoded = crate::settings::decode_stored_settings(&serde_json::Value::String(merged))?;
    if let Ok(mut cached) = app_state.scheduler_settings.write() {
        *cached = Some(decoded);
    }
    Ok(())
}

#[tauri::command]
fn db_load_settings(state: tauri::State<'_, crate::db::DbState>) -> Result<Option<String>, String> {
    let connection = state.lock()?;
    crate::db::load_settings(&connection)
}

#[tauri::command]
fn db_get_all_downloads(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<Vec<String>, String> {
    let connection = state.lock()?;
    crate::db::load_downloads(&connection)
}

#[tauri::command]
fn db_replace_downloads(
    state: tauri::State<'_, crate::db::DbState>,
    data: String,
) -> Result<(), String> {
    let mut connection = state.lock()?;
    crate::db::replace_downloads(&mut connection, &data)
}

#[tauri::command]
fn db_get_all_queues(state: tauri::State<'_, crate::db::DbState>) -> Result<Vec<String>, String> {
    let connection = state.lock()?;
    crate::db::load_queues(&connection)
}

#[tauri::command]
fn db_replace_queues(
    state: tauri::State<'_, crate::db::DbState>,
    data: String,
) -> Result<(), String> {
    let mut connection = state.lock()?;
    crate::db::replace_queues(&mut connection, &data)
}

#[tauri::command]
fn check_file_exists(app_handle: tauri::AppHandle, path: String) -> bool {
    let resolved_dest = resolve_path(&path, &app_handle);
    if !is_safe_path(&resolved_dest, &app_handle) {
        return false;
    }
    resolved_dest.exists()
}

async fn log_files(app_handle: &tauri::AppHandle) -> Result<Vec<std::path::PathBuf>, String> {
    use tauri::Manager;
    let log_dir = app_handle.path().app_log_dir().map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&log_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains(".log"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn redact_log_line(line: &str) -> String {
    use std::sync::OnceLock;
    static SECRET: OnceLock<regex::Regex> = OnceLock::new();
    static HEADER: OnceLock<regex::Regex> = OnceLock::new();
    static QUERY: OnceLock<regex::Regex> = OnceLock::new();
    let secret = SECRET.get_or_init(|| {
        regex::Regex::new(r"(?i)(authorization|cookie|password|token|secret)\s*[:=]\s*([^\s,;]+)")
            .expect("valid secret redaction regex")
    });
    let header = HEADER.get_or_init(|| {
        regex::Regex::new(r"(?i)(authorization|cookie)\s*:\s*[^\r\n]+")
            .expect("valid sensitive header redaction regex")
    });
    let query = QUERY.get_or_init(|| {
        regex::Regex::new(r"(https?://[^\s?]+)\?[^\s]+").expect("valid URL query redaction regex")
    });
    let redacted = header.replace_all(line, "$1: [redacted]");
    let redacted = secret.replace_all(&redacted, "$1=[redacted]");
    query.replace_all(&redacted, "$1?[redacted]").into_owned()
}

fn redact_log_line_for_output(line: &str) -> String {
    static HOME_PATHS: OnceLock<(String, String)> = OnceLock::new();
    let (home, escaped_home) = HOME_PATHS.get_or_init(|| {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let home = if home.len() > 3 { home } else { String::new() };
        let escaped_home = home.replace('\\', "\\\\");
        (home, escaped_home)
    });

    let mut redacted = line.to_string();
    if !home.is_empty() {
        redacted = redacted.replace(home, "<HOME>");
    }
    if !escaped_home.is_empty() && escaped_home != home {
        redacted = redacted.replace(escaped_home, "<HOME>");
    }
    redact_log_line(&redacted)
}

fn redact_log_line_for_app(line: &str, app_handle: &tauri::AppHandle) -> String {
    use tauri::Manager;
    let without_home = app_handle
        .path()
        .home_dir()
        .ok()
        .map(|home| {
            let home = home.to_string_lossy();
            line.replace(home.as_ref(), "~")
        })
        .unwrap_or_else(|| line.to_string());
    redact_log_line(&without_home)
}

#[tauri::command]
async fn read_logs(app_handle: tauri::AppHandle, limit: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for file in log_files(&app_handle).await? {
        let content = tokio::fs::read_to_string(&file)
            .await
            .map_err(|error| format!("failed to read '{}': {error}", file.display()))?;
        lines.extend(
            content
                .lines()
                .map(|line| redact_log_line_for_app(line, &app_handle)),
        );
    }
    let keep = limit.clamp(1, 10_000);
    if lines.len() > keep {
        lines.drain(..lines.len() - keep);
    }
    Ok(lines)
}

#[tauri::command]
async fn clear_logs(app_handle: tauri::AppHandle) -> Result<(), String> {
    for file in log_files(&app_handle).await? {
        tokio::fs::write(&file, "")
            .await
            .map_err(|e| format!("Failed to clear log file {:?}: {}", file, e))?;
    }
    Ok(())
}

#[tauri::command]
async fn export_logs(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut output = format!(
        "Firelink support logs\nVersion: {}\nOS: {} {}\nArchitecture: {}\nGenerated: {}\n\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::FAMILY,
        std::env::consts::ARCH,
        chrono::Utc::now().to_rfc3339(),
    );
    let (aria2, ytdlp, ffmpeg, deno) = tokio::join!(
        check_aria2(
            &app_handle,
            state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
            &state.aria2_secret
        ),
        check_ytdlp(&app_handle),
        check_ffmpeg(&app_handle),
        check_deno(&app_handle),
    );
    output.push_str("Engine status:\n");
    for engine in [aria2, ytdlp, ffmpeg, deno] {
        output.push_str(&format!(
            "- {}: {}{}\n",
            engine.name,
            if engine.ready { "ready" } else { "unavailable" },
            engine
                .version
                .as_deref()
                .map(|version| format!(" ({version})"))
                .unwrap_or_default()
        ));
        if let Some(error) = engine.error {
            output.push_str(&format!("  Error: {}\n", redact_log_line(&error)));
        }
    }
    if let Ok(settings) = crate::settings::load_settings(&app_handle) {
        output.push_str(&format!(
            "\nRuntime settings:\n- Max concurrent downloads: {}\n- Per-server connections: {}\n- Automatic retries: {}\n- Proxy mode: {:?}\n- Scheduler enabled: {}\n\n",
            settings.max_concurrent_downloads,
            settings.per_server_connections,
            settings.max_automatic_retries,
            settings.proxy_mode,
            settings.scheduler.enabled,
        ));
    }
    for file in log_files(&app_handle).await? {
        let file_name = file
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("firelink.log"));
        output.push_str(&format!("===== {} =====\n", file_name));
        let content = tokio::fs::read_to_string(&file)
            .await
            .map_err(|error| format!("failed to read '{}': {error}", file.display()))?;
        for line in content.lines() {
            output.push_str(&redact_log_line_for_app(line, &app_handle));
            output.push('\n');
        }
        output.push('\n');
    }
    Ok(output)
}

#[tauri::command]
fn toggle_tray_icon(app_handle: tauri::AppHandle, show: bool) -> Result<(), String> {
    if show {
        build_main_tray(&app_handle)
    } else {
        if app_handle.tray_by_id("main").is_some() {
            let _ = app_handle.remove_tray_by_id("main");
        }
        Ok(())
    }
}

fn build_main_tray(app_handle: &tauri::AppHandle) -> Result<(), String> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    if app_handle.tray_by_id("main").is_some() {
        return Ok(());
    }

    let show_i = MenuItem::with_id(app_handle, "show", "Show Firelink", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let pause_all_i = MenuItem::with_id(app_handle, "pause_all", "Pause All", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let resume_all_i =
        MenuItem::with_id(app_handle, "resume_all", "Resume All", true, None::<&str>)
            .map_err(|e| e.to_string())?;
    let quit_i = MenuItem::with_id(app_handle, "quit", "Quit", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app_handle, &[&show_i, &pause_all_i, &resume_all_i, &quit_i])
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    let tray_icon_bytes = include_bytes!("../icons/trayTemplate.png").as_slice();
    #[cfg(not(target_os = "macos"))]
    let tray_icon_bytes = include_bytes!("../icons/128x128.png").as_slice();
    let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes).map_err(|e| e.to_string())?;
    #[allow(unused_mut)]
    let mut tray = TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => restore_main_window(app),
            "pause_all" => {
                use tauri::Emitter;
                let _ = app.emit("tray-action", "pause-all");
            }
            "resume_all" => {
                use tauri::Emitter;
                let _ = app.emit("tray-action", "resume-all");
            }
            "quit" => app.exit(0),
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
        });
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon_as_template(true);
    }
    tray.build(app_handle).map_err(|e| e.to_string())?;

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
fn get_extension_server_port(state: tauri::State<'_, AppState>) -> Option<u16> {
    state
        .extension_server_port
        .read()
        .ok()
        .and_then(|port| *port)
}

#[tauri::command]
fn set_extension_frontend_ready(state: tauri::State<'_, AppState>, ready: bool) {
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
    use super::{
        aggregate_media_fraction, append_ytdlp_config_option, append_ytdlp_http_headers,
        build_media_format_options,
        collect_download_uris, drain_media_output_lines, filename_from_content_disposition,
        filename_from_url_disposition_query, filename_from_url_path, is_excluded_yt_dlp_format,
        is_browser_cookie_extraction_error, json_lower, media_metadata_cache_key,
        media_output_template, media_progress_args, media_progress_speed,
        normalize_speed_limit_for_aria2,
        parse_firelink_deep_link, parse_ffmpeg_version, parse_media_progress_line,
        redact_log_line, redact_log_line_for_output, sanitize_ytdlp_config_value,
        should_cleanup_media_artifacts_after_failure, FirelinkDeepLink, MediaProgress,
        MediaSpeedSampler, MEDIA_PROGRESS_PREFIX,
    };
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    fn media_metadata_fallback_lets_ytdlp_choose_extension() {
        let destination = std::path::Path::new("/tmp/firelink");
        let template = media_output_template(destination, "1234567890", None);
        assert_eq!(
            template,
            destination.join("%(title).200B [%(id)s].%(ext)s")
        );
    }

    #[test]
    fn selected_media_format_keeps_requested_output_path() {
        let destination = std::path::Path::new("/tmp/firelink");
        let template = media_output_template(destination, "clip.mp4", Some("best"));
        assert_eq!(template, destination.join("clip.mp4"));
    }

    #[test]
    fn ytdlp_progress_args_force_progress_in_quiet_print_mode() {
        let args = media_progress_args();

        assert!(args.iter().any(|arg| arg == "--progress"));
        assert!(args.windows(2).any(|pair| {
            pair[0] == "--progress-template"
                && pair[1] == format!("download:{MEDIA_PROGRESS_PREFIX}%(progress)j")
        }));
    }

    #[test]
    fn ytdlp_config_values_cannot_inject_extra_lines() {
        assert_eq!(
            sanitize_ytdlp_config_value("user\n--exec\rmalicious"),
            "user--execmalicious"
        );
    }

    #[test]
    fn ytdlp_media_headers_include_captured_cookies_once() {
        let mut config = String::new();
        append_ytdlp_http_headers(
            &mut config,
            Some("Referer: https://example.com/video"),
            Some("session=abc; preference=high\r\n--proxy=http://bad.invalid"),
        )
        .unwrap();

        assert_eq!(
            config,
            "--add-header 'Referer: https://example.com/video'\n--add-header 'Cookie: session=abc; preference=high--proxy=http://bad.invalid'\n"
        );
    }

    #[test]
    fn ytdlp_config_options_quote_embedded_single_quotes() {
        let mut config = String::new();
        append_ytdlp_config_option(&mut config, "--username", "sam's account");

        assert_eq!(config, "--username 'sam'\\''s account'\n");
    }

    #[test]
    fn ytdlp_media_headers_reject_invalid_lines() {
        let mut config = String::new();
        let error = append_ytdlp_http_headers(&mut config, Some("not a header"), None)
            .expect_err("invalid header line should be rejected");

        assert!(error.contains("invalid HTTP header"));
    }

    #[test]
    fn media_metadata_cache_key_includes_request_headers_and_cookies() {
        let base = media_metadata_cache_key(
            "https://example.com/watch?v=1",
            &Some("firefox".to_string()),
            &Some("Custom UA A".to_string()),
            &None,
            &None,
            &Some("User-Agent: Browser A".to_string()),
            &Some("session=one".to_string()),
            &None,
        );
        let changed_headers = media_metadata_cache_key(
            "https://example.com/watch?v=1",
            &Some("firefox".to_string()),
            &Some("Custom UA A".to_string()),
            &None,
            &None,
            &Some("User-Agent: Browser B".to_string()),
            &Some("session=one".to_string()),
            &None,
        );
        let changed_cookies = media_metadata_cache_key(
            "https://example.com/watch?v=1",
            &Some("firefox".to_string()),
            &Some("Custom UA A".to_string()),
            &None,
            &None,
            &Some("User-Agent: Browser A".to_string()),
            &Some("session=two".to_string()),
            &None,
        );
        let changed_user_agent = media_metadata_cache_key(
            "https://example.com/watch?v=1",
            &Some("firefox".to_string()),
            &Some("Custom UA B".to_string()),
            &None,
            &None,
            &Some("User-Agent: Browser A".to_string()),
            &Some("session=one".to_string()),
            &None,
        );

        assert_ne!(base, changed_headers);
        assert_ne!(base, changed_cookies);
        assert_ne!(base, changed_user_agent);
    }

    #[test]
    fn retryable_media_failures_preserve_resumable_artifacts() {
        assert!(!should_cleanup_media_artifacts_after_failure(
            "The response status is not successful. status=503",
            0,
            1
        ));
        assert!(should_cleanup_media_artifacts_after_failure(
            "The response status is not successful. status=503",
            1,
            1
        ));
        assert!(should_cleanup_media_artifacts_after_failure(
            "HTTP 404 Not Found",
            0,
            3
        ));
    }

    #[test]
    fn metadata_filename_prefers_content_disposition_filename() {
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename*=UTF-8''OnionHop-3.5-macOS-arm64.dmg; filename=ignored.bin"
            ),
            Some("OnionHop-3.5-macOS-arm64.dmg".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=OnionHop-3.5-macOS-arm64.dmg"),
            Some("OnionHop-3.5-macOS-arm64.dmg".to_string())
        );
    }

    #[test]
    fn metadata_filename_reads_redirect_disposition_query_before_opaque_path() {
        let redirected = "https://release-assets.githubusercontent.com/github-production-release-asset/1117828249/7aae36e6-00ec-4e7d-8dec-f14ace170bdb?rscd=attachment%3B+filename%3DOnionHop-3.5-macOS-arm64.dmg";

        assert_eq!(
            filename_from_url_disposition_query(redirected),
            Some("OnionHop-3.5-macOS-arm64.dmg".to_string())
        );
        assert_eq!(
            filename_from_url_path(redirected),
            Some("7aae36e6-00ec-4e7d-8dec-f14ace170bdb".to_string())
        );
    }

    #[test]
    fn normalizes_bare_global_speed_limits_as_kib_per_second() {
        assert_eq!(
            normalize_speed_limit_for_aria2("1024"),
            Some("1024K".to_string())
        );
        assert_eq!(
            normalize_speed_limit_for_aria2("512K"),
            Some("512K".to_string())
        );
        assert_eq!(
            normalize_speed_limit_for_aria2("1.5 MB/s"),
            Some("1.5M".to_string())
        );
        assert_eq!(normalize_speed_limit_for_aria2("0"), None);
        assert_eq!(normalize_speed_limit_for_aria2("bad"), None);
    }

    #[test]
    fn redacts_secrets_and_signed_url_queries_from_support_logs() {
        let line =
            "Authorization: bearer-secret Cookie=session=abc https://example.com/file?token=secret";
        let redacted = redact_log_line(line);
        assert!(!redacted.contains("bearer-secret"));
        assert!(!redacted.contains("session=abc"));
        assert!(!redacted.contains("token=secret"));
        assert!(redacted.contains("[redacted]"));
    }

    #[test]
    fn redacts_live_log_output_before_webview_delivery() {
        let line = "Cookie: session=abc https://example.com/file?signature=secret";
        let redacted = redact_log_line_for_output(line);
        assert!(!redacted.contains("session=abc"));
        assert!(!redacted.contains("signature=secret"));
        assert!(redacted.contains("[redacted]"));
    }

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
            parse_firelink_deep_link(&deep_link),
            FirelinkDeepLink::Add(vec![
                "https://example.com/one.zip".to_string(),
                "ftp://example.com/two.zip".to_string(),
            ])
        );
    }

    #[test]
    fn accepts_exact_launch_without_downloads() {
        let deep_link = url::Url::parse("firelink://launch").unwrap();
        assert_eq!(
            parse_firelink_deep_link(&deep_link),
            FirelinkDeepLink::Launch
        );
    }

    #[test]
    fn rejects_launch_variants_and_nested_schemes() {
        let links = [
            url::Url::parse("firelink://open?url=https%3A%2F%2Fexample.com").unwrap(),
            url::Url::parse("firelink://add?url=file%3A%2F%2F%2Ftmp%2Fsecret").unwrap(),
            url::Url::parse("other://add?url=https%3A%2F%2Fexample.com").unwrap(),
            url::Url::parse("firelink://launch?url=https%3A%2F%2Fexample.com").unwrap(),
            url::Url::parse("firelink://launch/path").unwrap(),
            url::Url::parse("firelink://user@launch").unwrap(),
            url::Url::parse("firelink://add/path?url=https%3A%2F%2Fexample.com").unwrap(),
        ];

        assert!(links
            .iter()
            .all(|link| parse_firelink_deep_link(link) == FirelinkDeepLink::Invalid));
    }

    #[test]
    fn excludes_youtube_storyboard_mhtml_formats() {
        let storyboard = json!({
            "format_id": "sb0",
            "ext": "mhtml",
            "protocol": "mhtml",
            "format_note": "storyboard",
            "vcodec": "none",
            "acodec": "none"
        });

        assert!(is_excluded_yt_dlp_format(&storyboard));
    }

    #[test]
    fn builds_compact_media_options_without_storyboards() {
        let formats = vec![
            json!({
                "format_id": "sb0",
                "ext": "mhtml",
                "protocol": "mhtml",
                "format_note": "storyboard",
                "vcodec": "none",
                "acodec": "none"
            }),
            json!({
                "format_id": "137",
                "ext": "mp4",
                "height": 1080,
                "format_note": "1080p",
                "vcodec": "avc1.640028",
                "acodec": "none",
                "filesize": 100_000_000_u64
            }),
            json!({
                "format_id": "140",
                "ext": "m4a",
                "vcodec": "none",
                "acodec": "mp4a.40.2",
                "filesize": 10_000_000_u64
            }),
        ];

        let options = build_media_format_options(&formats, Some(600.0));

        assert!(!options.iter().any(|format| format.ext == "mhtml"));
        assert!(!options.iter().any(|format| format.resolution == "Best"));
        assert!(!options.iter().any(|format| format.resolution == "1440p"));
        assert!(options.iter().any(|format| {
            format.resolution == "1080p"
                && format.ext == "mkv"
                && format.format_label == "MKV • H.264 + AAC"
                && format.filesize == Some(110_000_000)
        }));
        assert!(options.iter().any(|format| {
            format.resolution == "1080p"
                && format.ext == "mp4"
                && format.format_label == "MP4 • H.264 + AAC"
                && format.filesize == Some(110_000_000)
        }));
        assert!(options.iter().any(|format| {
            format.resolution == "Audio only" && format.format_label == "M4A • AAC"
        }));
    }

    #[test]
    fn estimates_missing_video_size_from_bitrate_and_uses_exact_stream_ids() {
        let formats = vec![
            json!({
                "format_id": "301",
                "ext": "mp4",
                "height": 1080,
                "fps": 60,
                "vcodec": "avc1.64002A",
                "acodec": "none",
                "tbr": 6_400.0
            }),
            json!({
                "format_id": "251",
                "ext": "webm",
                "vcodec": "none",
                "acodec": "opus",
                "filesize": 28_000_000_u64
            }),
        ];
        let options = build_media_format_options(&formats, Some(1_800.0));
        let option = options
            .iter()
            .find(|format| format.resolution == "1080p" && format.ext == "mkv")
            .expect("1080p MKV option");

        assert_eq!(option.format_id, "301+251");
        assert_eq!(option.filesize, None);
        assert_eq!(option.filesize_approx, Some(1_468_000_000));
    }

    #[test]
    fn keeps_every_available_video_height_including_nonstandard_qualities() {
        let formats = vec![
            json!({
                "format_id": "401",
                "ext": "webm",
                "height": 4320,
                "vcodec": "av01.0.17M.08",
                "acodec": "none"
            }),
            json!({
                "format_id": "17",
                "ext": "mp4",
                "height": 144,
                "vcodec": "avc1.42E01E",
                "acodec": "mp4a.40.2"
            }),
            json!({
                "format_id": "five-k",
                "ext": "webm",
                "resolution": "5120X2880",
                "vcodec": "av01.0.17M.08",
                "acodec": "none"
            }),
            json!({
                "format_id": "pal",
                "ext": "mp4",
                "format_note": "Premium 576p",
                "vcodec": "avc1.4d401f",
                "acodec": "none"
            }),
        ];

        let options = build_media_format_options(&formats, Some(60.0));

        assert!(options.iter().any(|format| format.resolution == "4320p"));
        assert!(options.iter().any(|format| format.resolution == "2880p"));
        assert!(options.iter().any(|format| format.resolution == "576p"));
        assert!(options.iter().any(|format| format.resolution == "144p"));
    }

    #[test]
    fn returns_no_media_options_for_webpage_only_metadata() {
        let formats = vec![json!({
            "format_id": "0",
            "ext": "html",
            "protocol": "https",
            "format": "default webpage",
            "vcodec": "none",
            "acodec": "none"
        })];

        assert!(build_media_format_options(&formats, Some(30.0)).is_empty());
    }

    #[test]
    fn classifies_browser_cookie_database_errors_for_fallback() {
        assert!(is_browser_cookie_extraction_error(
            "ERROR: Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271"
        ));
        assert!(is_browser_cookie_extraction_error(
            "failed to read browser cookie data"
        ));
        assert!(!is_browser_cookie_extraction_error(
            "ERROR: Sign in to confirm you are not a bot"
        ));
        assert!(!is_browser_cookie_extraction_error(
            "ERROR: requested format is not available"
        ));
    }

    #[test]
    #[ignore = "requires network and a local yt-dlp executable"]
    fn filters_live_youtube_metadata_from_env() {
        let url = std::env::var("FIRELINK_LIVE_YOUTUBE_URL")
            .expect("set FIRELINK_LIVE_YOUTUBE_URL to a YouTube watch URL");
        let output = std::process::Command::new("yt-dlp")
            .args([
                "--dump-json",
                "--no-warnings",
                "--no-playlist",
                "--socket-timeout",
                "20",
                "--retries",
                "3",
                "--extractor-retries",
                "3",
                "--compat-options",
                "no-youtube-unavailable-videos",
                "--",
                &url,
            ])
            .output()
            .expect("failed to run yt-dlp");

        assert!(
            output.status.success(),
            "yt-dlp failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("yt-dlp did not emit valid JSON");
        let formats = value
            .get("formats")
            .and_then(|v| v.as_array())
            .expect("yt-dlp JSON did not include formats");
        let raw_mhtml_count = formats
            .iter()
            .filter(|format| json_lower(format, "ext") == "mhtml")
            .count();
        let duration = value.get("duration").and_then(|duration| duration.as_f64());
        let options = build_media_format_options(formats, duration);

        eprintln!(
            "raw formats: {}, raw mhtml: {}, normalized options: {}",
            formats.len(),
            raw_mhtml_count,
            options.len()
        );
        assert!(!options.is_empty());
        assert!(options.iter().all(|format| format.ext != "mhtml"));
        assert!(options
            .iter()
            .all(|format| !format.format_label.to_lowercase().contains("mhtml")));
    }

    #[test]
    fn parses_structured_ytdlp_progress() {
        let line = format!(
            "{MEDIA_PROGRESS_PREFIX}{{\"downloaded_bytes\":5242880,\"total_bytes\":10485760,\"speed\":1048576,\"eta\":5,\"_speed_str\":\"1.00MiB/s\",\"_eta_str\":\"00:05\",\"_total_bytes_str\":\"10.00MiB\"}}"
        );

        assert_eq!(
            parse_media_progress_line(&line),
            Some(MediaProgress {
                fraction: 0.5,
                speed: "1.00MiB/s".to_string(),
                eta: "00:05".to_string(),
                size: Some("10.00MiB".to_string()),
                downloaded_bytes: Some(5242880.0),
            })
        );
    }

    #[test]
    fn parses_chunked_structured_ytdlp_progress() {
        let mut buffer = String::new();
        let first = format!("{MEDIA_PROGRESS_PREFIX}{{\"downloaded_bytes\":5242880,");
        let second = "\"total_bytes\":10485760,\"_percent\":50.0,\"_speed_str\":\"1.00MiB/s\"}\n";

        assert!(drain_media_output_lines(&mut buffer, &first).is_empty());
        let lines = drain_media_output_lines(&mut buffer, second);

        assert_eq!(lines.len(), 1);
        assert_eq!(
            parse_media_progress_line(&lines[0]).map(|progress| progress.fraction),
            Some(0.5)
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_structured_ytdlp_numeric_percent_without_total() {
        let line = format!(
            "{MEDIA_PROGRESS_PREFIX}{{\"downloaded_bytes\":5242880,\"_percent\":37.5,\"_speed_str\":\"1.00MiB/s\"}}"
        );

        assert_eq!(
            parse_media_progress_line(&line).map(|progress| progress.fraction),
            Some(0.375)
        );
    }

    #[test]
    fn parses_ffmpeg_snapshot_version_without_collapsing_to_n() {
        let output = "ffmpeg version N-125385-ge2e889d9da-https://www.martin-riedl.de Copyright (c) 2000-2026 the FFmpeg developers";

        assert_eq!(
            parse_ffmpeg_version(output),
            Some("N-125385-ge2e889d9da".to_string())
        );
    }

    #[test]
    fn parses_ffmpeg_release_version_without_build_suffix() {
        let output = "ffmpeg version 8.1.2-static https://example.invalid Copyright (c) 2000-2026 the FFmpeg developers";

        assert_eq!(parse_ffmpeg_version(output), Some("8.1.2".to_string()));
    }

    #[test]
    fn uses_fragment_progress_instead_of_temporary_hls_size_estimates() {
        let line = format!(
            "{MEDIA_PROGRESS_PREFIX}{{\"downloaded_bytes\":1024,\"total_bytes_estimate\":1024,\"fragment_index\":0,\"fragment_count\":354,\"_percent_str\":\"100.0%\"}}"
        );

        assert_eq!(
            parse_media_progress_line(&line).map(|progress| progress.fraction),
            Some(0.0)
        );
    }

    #[test]
    fn advances_tracks_only_after_a_completed_track_restarts() {
        let mut current_track = 0.0;
        let mut last_fraction = 0.0;

        assert_eq!(
            aggregate_media_fraction(2.0, &mut current_track, &mut last_fraction, 0.5),
            0.25
        );
        assert_eq!(
            aggregate_media_fraction(2.0, &mut current_track, &mut last_fraction, 1.0),
            0.5
        );
        assert_eq!(
            aggregate_media_fraction(2.0, &mut current_track, &mut last_fraction, 0.0),
            0.5
        );
        assert_eq!(
            aggregate_media_fraction(2.0, &mut current_track, &mut last_fraction, 0.4),
            0.7
        );
        assert_eq!(current_track, 1.0);
    }

    #[test]
    fn derives_main_window_speed_from_downloaded_byte_delta() {
        let first = MediaProgress {
            fraction: 0.25,
            speed: "fallback".to_string(),
            eta: "-".to_string(),
            size: None,
            downloaded_bytes: Some(1_000_000.0),
        };
        let second = MediaProgress {
            fraction: 0.5,
            speed: "fallback".to_string(),
            eta: "-".to_string(),
            size: None,
            downloaded_bytes: Some(3_097_152.0),
        };
        let start = Instant::now();
        let mut sampler = MediaSpeedSampler::default();

        assert_eq!(
            media_progress_speed(&first, start, &mut sampler),
            ("fallback".to_string(), "-".to_string())
        );
        assert_eq!(
            media_progress_speed(&second, start + Duration::from_secs(1), &mut sampler),
            ("2.0 MB/s".to_string(), "1s".to_string())
        );
    }

    #[test]
    fn smooths_media_speed_across_short_stalls() {
        let start = Instant::now();
        let mut sampler = MediaSpeedSampler::default();
        let mut progress = MediaProgress {
            fraction: 0.25,
            speed: "-".to_string(),
            eta: "-".to_string(),
            size: None,
            downloaded_bytes: Some(1_000_000.0),
        };

        assert_eq!(
            media_progress_speed(&progress, start, &mut sampler),
            ("-".to_string(), "-".to_string())
        );

        progress.fraction = 0.5;
        progress.downloaded_bytes = Some(3_000_000.0);
        assert_eq!(
            media_progress_speed(&progress, start + Duration::from_secs(1), &mut sampler),
            ("1.9 MB/s".to_string(), "2s".to_string())
        );

        progress.downloaded_bytes = Some(3_000_000.0);
        assert_eq!(
            media_progress_speed(&progress, start + Duration::from_secs(2), &mut sampler),
            ("976.6 KB/s".to_string(), "3s".to_string())
        );
    }

    #[test]
    fn parses_aria2_external_downloader_progress() {
        let line = "[#2d2636 12MiB/34MiB(34%) CN:1 DL:910KiB ETA:25s]";

        assert_eq!(
            parse_media_progress_line(line),
            Some(MediaProgress {
                fraction: 0.34,
                speed: "910KiB/s".to_string(),
                eta: "25s".to_string(),
                size: Some("34MiB".to_string()),
                downloaded_bytes: None,
            })
        );
    }

    #[test]
    fn retains_legacy_ytdlp_progress_fallback() {
        let line = "[download]  42.5% of 10.00MiB at 2.00MiB/s ETA 00:03";

        assert_eq!(
            parse_media_progress_line(line),
            Some(MediaProgress {
                fraction: 0.425,
                speed: "2.00MiB/s".to_string(),
                eta: "00:03".to_string(),
                size: Some("10.00MiB".to_string()),
                downloaded_bytes: Some(4456448.0),
            })
        );
    }
}

static LOG_PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static LOG_STREAM_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn toggle_log_pause(pause: bool) {
    LOG_PAUSED.store(pause, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
fn is_log_paused() -> bool {
    LOG_PAUSED.load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
fn set_log_stream_active(active: bool) {
    LOG_STREAM_ACTIVE.store(active, std::sync::atomic::Ordering::Release);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_reqwest_crypto_provider();

    let extension_pairing_token = Arc::new(RwLock::new(String::new()));
    let server_pairing_token = extension_pairing_token.clone();
    let extension_frontend_ready = Arc::new(AtomicBool::new(false));
    let server_frontend_ready = extension_frontend_ready.clone();
    let extension_server_port = Arc::new(RwLock::new(None));
    let server_extension_port = extension_server_port.clone();
    let (extension_server_shutdown_tx, extension_server_shutdown_rx) =
        tokio::sync::watch::channel(false);

    let initial_aria2_port = 6800; // Will be determined dynamically in background
    let aria2_port = Arc::new(std::sync::atomic::AtomicU16::new(initial_aria2_port));
    let aria2_port_clone = Arc::clone(&aria2_port);
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
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                window
                    .set_decorations(false)
                    .map_err(|error| format!("failed to disable Windows native frame: {error}"))?;
            }

            let mut sys = sysinfo::System::new_all();
            sys.refresh_all();
            log::info!("=== System Information ===");
            log::info!("OS: {} {}", sysinfo::System::name().unwrap_or_else(|| "Unknown".to_string()), sysinfo::System::os_version().unwrap_or_else(|| "Unknown".to_string()));
            let arch = sysinfo::System::cpu_arch();
            log::info!("Architecture: {}", if arch.is_empty() { "Unknown" } else { &arch });
            log::info!("CPU: {} ({} cores)", sys.cpus().first().map(|c| c.brand()).unwrap_or("Unknown"), sys.cpus().len());
            log::info!("Memory: {} MB total", sys.total_memory() / 1024 / 1024);
            log::info!("App Version: {}", env!("CARGO_PKG_VERSION"));
            log::info!("==========================");
            build_main_tray(app.handle())
                .map_err(|error| format!("failed to create tray menu: {error}"))?;

            let database = crate::db::init(app.handle())
                .map_err(|error| format!("failed to initialize persistence: {error}"))?;
            let initial_pairing_token = {
                // Generate a temporary session token for the extension server on startup.
                // The frontend will hydrate the real token via IPC once it mounts,
                // avoiding any macOS system prompts before the UI is fully visible.
                format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                )
            };
            {
                let mut pairing_token = extension_pairing_token
                    .write()
                    .map_err(|_| "extension pairing token lock is unavailable".to_string())?;
                *pairing_token = initial_pairing_token;
            }
            app.manage(database);
            let persisted_settings = crate::settings::load_settings(app.handle()).ok();
            let logs_enabled = persisted_settings
                .as_ref()
                .is_some_and(|settings| settings.logs_enabled);
            LOG_PAUSED.store(!logs_enabled, std::sync::atomic::Ordering::Relaxed);
            if logs_enabled {
                log::info!("=== System Information ===");
                log::info!(
                    "OS: {} {}",
                    sysinfo::System::name().unwrap_or_else(|| "Unknown".to_string()),
                    sysinfo::System::os_version().unwrap_or_else(|| "Unknown".to_string())
                );
                let arch = sysinfo::System::cpu_arch();
                log::info!(
                    "Architecture: {}",
                    if arch.is_empty() { "Unknown" } else { &arch }
                );
                log::info!(
                    "CPU: {} ({} cores)",
                    sys.cpus().first().map(|c| c.brand()).unwrap_or("Unknown"),
                    sys.cpus().len()
                );
                log::info!("Memory: {} MB total", sys.total_memory() / 1024 / 1024);
                log::info!("App Version: {}", env!("CARGO_PKG_VERSION"));
                log::info!("==========================");
            }

            let max_concurrent = {
                persisted_settings
                    .as_ref()
                    .map(|settings| settings.max_concurrent_downloads)
                    .unwrap_or(crate::queue::DEFAULT_MAX_CONCURRENT)
            };
            let scheduler_settings = Arc::new(RwLock::new(persisted_settings.clone()));

            let queue_manager = Arc::new(queue::QueueManager::new(app.handle().clone(), max_concurrent));
            let dispatcher_mgr = Arc::clone(&queue_manager);
            tauri::async_runtime::spawn(async move {
                dispatcher_mgr.run_dispatcher().await;
            });

            let queue_manager_poll = Arc::clone(&queue_manager);

            app.manage(AppState {
                download_coordinator: download::DownloadCoordinator::spawn(app.handle().clone()),
                extension_pairing_token,
                extension_frontend_ready,
                extension_server_port,
                extension_server_shutdown: extension_server_shutdown_tx.clone(),
                aria2_port: aria2_port.clone(),
                aria2_secret: aria2_secret.clone(),
                media_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
                sleep_preventer: Arc::new(Mutex::new(None)),
                scheduler_settings: Arc::clone(&scheduler_settings),
                queue_manager,
            });

            let deep_link_app = app.handle().clone();
            #[cfg(target_os = "linux")]
            if let Err(error) = app.deep_link().register_all() {
                log::warn!("Could not register firelink:// handler: {error}");
            }
            app.deep_link().on_open_url(move |event| {
                dispatch_deep_links(deep_link_app.clone(), event.urls());
            });
            match app.deep_link().get_current() {
                Ok(Some(urls)) => dispatch_deep_links(app.handle().clone(), urls),
                Ok(None) => {}
                Err(error) => eprintln!("Failed to read startup deep link: {error}"),
            }
            crate::scheduler::spawn_scheduler(app.handle().clone(), scheduler_settings);

            let global_speed_limit = persisted_settings
                .as_ref()
                .map(|settings| settings.global_speed_limit.clone())
                .unwrap_or_default();

            let aria2_secret_clone = aria2_secret.clone();
            let app_handle_bg = app.handle().clone();
            tauri::async_runtime::spawn(async move {

                let mut ws_port = 6800;
                match resolve_bundled_binary_path(&app_handle_bg, "aria2c") {
                    Ok(binary_path) => {
                        let mut success = false;
                        for attempt_port in 6800..6900 {
                            let mut cmd = std::process::Command::new(&binary_path);
                            crate::platform::hide_child_console(&mut cmd);
                            crate::engines::apply_aria2_environment(&mut cmd, &binary_path);

                            let mut config_file = tempfile::Builder::new().prefix("aria2-").suffix(".conf").tempfile().expect("failed to create aria2 config file");
                            use std::io::Write;
                            let config_content = format!("rpc-secret={}\n", aria2_secret_clone);
                            config_file.write_all(config_content.as_bytes()).expect("failed to write aria2 config file");
                            let config_path = config_file.into_temp_path();

                            cmd.arg("--enable-rpc=true")
                                .arg(format!("--conf-path={}", config_path.display()))
                                .arg(format!("--rpc-listen-port={}", attempt_port))
                                .arg("--rpc-listen-all=false")
                                .arg("--continue=true")
                                .arg("--retry-wait=2")
                                .arg("--allow-overwrite=false")
                                .arg("--summary-interval=1")
                                .arg("--console-log-level=warn")
                                .arg("--download-result=hide")
                                .arg("--max-concurrent-downloads=9999")
                                .arg("--check-certificate=true")
                                .arg(format!("--stop-with-process={}", std::process::id()));

                            if let Some(limit) = normalize_speed_limit_for_aria2(&global_speed_limit) {
                                cmd.arg(format!("--max-overall-download-limit={}", limit));
                            }

                            cmd.stdout(std::process::Stdio::null());
                            cmd.stderr(std::process::Stdio::piped());

                            match cmd.spawn() {
                                Ok(mut child) => {
                                    // Give it a moment to fail if port is in use
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                    if let Ok(Some(_)) = child.try_wait() {
                                        // Process exited, likely port collision, try next
                                        continue;
                                    }

                                    log::info!("aria2c spawned successfully on port {}", attempt_port);

                                    aria2_port_clone.store(attempt_port, std::sync::atomic::Ordering::Relaxed);
                                    ws_port = attempt_port;
                                    success = true;

                                    let daemon_app = app_handle_bg.clone();
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

                                    let guard = app_handle_bg.state::<Aria2DaemonGuard>();
                                    *guard.child.lock().unwrap() = Some(child);
                                    *guard.config_path.lock().unwrap() = Some(config_path);

                                    let mut last_err = String::new();
                                    let start = std::time::Instant::now();
                                    let mut ready = false;
                                    while start.elapsed() < std::time::Duration::from_secs(5) {
                                        match rpc_call(attempt_port, &aria2_secret_clone, "aria2.getVersion", serde_json::json!([])).await {
                                            Ok(ver) => {
                                                let v = ver.get("version").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                log::info!("aria2 daemon ready (version {}) on port {}", v, attempt_port);
                                                ready = true;
                                                break;
                                            }
                                            Err(e) => {
                                                last_err = e;
                                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                            }
                                        }
                                    }
                                    if !ready {
                                        let err = if last_err.is_empty() { "aria2 daemon did not become ready within 5 seconds".to_string() } else { format!("aria2 did not become ready: {last_err}") };
                                        log::error!("{}", err);
                                        *guard.startup_error.lock().unwrap() = Some(err);
                                    }
                                    break;
                                }
                                Err(e) => {
                                    log::error!("Failed to spawn aria2c: {}", e);
                                    let guard = app_handle_bg.state::<Aria2DaemonGuard>();
                                    *guard.startup_error.lock().unwrap() = Some(format!("Failed to spawn aria2c: {e}"));
                                    break;
                                }
                            }
                        }
                        if !success {
                            let guard = app_handle_bg.state::<Aria2DaemonGuard>();
                            *guard.startup_error.lock().unwrap() = Some("Failed to find open port for aria2c".to_string());
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to resolve aria2c binary: {}", e);
                        let guard = app_handle_bg.state::<Aria2DaemonGuard>();
                        *guard.startup_error.lock().unwrap() = Some(format!("Failed to resolve aria2c: {e}"));
                    }
                }

                let mut ws_retries = 0;
                loop {
                    if ws_retries > 10 {
                        log::error!("Max WebSocket reconnection attempts reached. aria2 integration is disabled.");
                        let guard = app_handle_bg.state::<Aria2DaemonGuard>();
                        *guard.startup_error.lock().unwrap() = Some("Max WebSocket reconnection attempts reached.".to_string());
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let ws_url = format!("ws://127.0.0.1:{}/jsonrpc", ws_port);
                    if let Ok((ws_stream, _)) = tokio_tungstenite::connect_async(&ws_url).await {
                        ws_retries = 0; // reset on success
                        use futures_util::StreamExt;
                        let (_, mut read) = ws_stream.split();
                        while let Some(msg) = read.next().await {
                            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                                        if let Some(params) = json.get("params").and_then(|p| p.as_array()) {
                                            if let Some(event) = params.first().and_then(|p| p.as_object()) {
                                                if let Some(gid) = event.get("gid").and_then(|g| g.as_str()) {
                                                    let state = app_handle_bg.state::<AppState>();
                                                    let outcome = match method {
                                                        "aria2.onDownloadComplete" => Some(crate::queue::PendingOutcome::Complete),
                                                        "aria2.onDownloadError" => {
                                                            let mut msg = event.get("error_message").and_then(|m| m.as_str()).unwrap_or("aria2 download error").to_string();
                                                            let aria2_port = state.aria2_port.load(std::sync::atomic::Ordering::Relaxed);
                                                            let aria2_secret = state.aria2_secret.clone();
                                                    if let Ok(status) = rpc_call(aria2_port, &aria2_secret, "aria2.tellStatus", serde_json::json!([gid, ["errorCode", "errorMessage"]])).await {
                                                        let err_msg = status
                                                            .get("errorMessage")
                                                            .and_then(|m| m.as_str())
                                                            .filter(|m| !m.is_empty());
                                                        let err_code = status
                                                            .get("errorCode")
                                                            .and_then(|m| m.as_str())
                                                            .filter(|m| !m.is_empty());
                                                        match (err_code, err_msg) {
                                                            (Some(code), Some(message)) => {
                                                                msg = format!("aria2 error code {code}: {message}");
                                                            }
                                                            (Some(code), None) => {
                                                                msg = format!("aria2 error code {code}: {msg}");
                                                            }
                                                            (None, Some(message)) => {
                                                                msg = message.to_string();
                                                            }
                                                            (None, None) => {}
                                                        }
                                                    }
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
                    ws_retries += 1;
                    app_handle_bg.state::<AppState>().queue_manager.clear_aria2_permits().await;
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                }
            });

            let app_handle_poll = app.handle().clone();
            let poll_port = aria2_port.clone();
            let poll_secret = aria2_secret.clone();
            let poll_mgr = Arc::clone(&queue_manager_poll);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                loop {
                    interval.tick().await;
                    let params = serde_json::json!([["gid", "status", "totalLength", "completedLength", "downloadSpeed", "errorMessage"]]);
                    if let Ok(active_list) = rpc_call(poll_port.load(std::sync::atomic::Ordering::Relaxed), &poll_secret, "aria2.tellActive", params).await {
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
                                    size_is_final: false,
                                });
                                }
                            }
                        }
                    }
                }
            });


            let ext_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Err(error) = extension_server::start_server(
                    ext_app_handle.clone(),
                    server_pairing_token.clone(),
                    server_frontend_ready.clone(),
                    server_extension_port.clone(),
                    extension_server_shutdown_rx.clone(),
                ).await {
                    log::error!("Browser extension server unavailable: {error}");
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            });
            Ok(())
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: None,
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview)
                        .filter(|_| {
                            LOG_STREAM_ACTIVE.load(std::sync::atomic::Ordering::Acquire)
                        }),
                ])
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .filter(|metadata| {
                    if LOG_PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
                        return false;
                    }
                    let target = metadata.target();
                    !target.starts_with("webview::")
                        && !target.starts_with("hyper")
                        && !target.starts_with("reqwest")
                        && !target.starts_with("rustls")
                        && !target.starts_with("h2")
                        && !target.starts_with("tower")
                })
                .max_file_size(10_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .format(|out, message, record| {
                    let redacted = redact_log_line_for_output(&message.to_string());
                    out.finish(format_args!(
                        "[{}][{}][{}] {}",
                        chrono::Local::now().format("%Y-%m-%d][%H:%M:%S"),
                        record.level(),
                        record.target(),
                        redacted
                    ))
                })
                .build(),
        )
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
 get_engine_status, get_aria2_engine_status, get_ytdlp_engine_status, get_ffmpeg_engine_status,
 get_deno_engine_status, test_ytdlp, test_aria2c, test_ffmpeg, test_deno,
 pause_download, resume_download, fetch_metadata, fetch_media_metadata,
            update_dock_badge, get_platform_info, approve_download_root, set_prevent_sleep, get_free_space, perform_system_action,
            ack_schedule_trigger,
            check_automation_permission, request_automation_permission, open_automation_settings,
            set_keychain_password, get_keychain_password, delete_keychain_password,
            hydrate_extension_pairing_token, grant_keychain_access, acknowledge_pairing_token_change,
            check_file_exists, toggle_tray_icon, set_extension_pairing_token,
            get_extension_server_port, set_extension_frontend_ready, set_concurrent_limit, set_global_speed_limit, remove_download,
            detach_download_for_reconfigure,
            enqueue_download, enqueue_many, cancel_enqueue_generation, move_in_queue, remove_from_queue, get_pending_order,
            commands::reveal_in_file_manager, commands::open_downloaded_file,
            parity::get_system_proxy, parity::get_file_category, parity::check_for_updates, parity::is_supported_media, parity::get_supported_media_domains,
            parity::create_category_directories,
            db_save_settings, db_load_settings, db_get_all_downloads, db_replace_downloads,
            db_get_all_queues, db_replace_queues,
            read_logs, export_logs, toggle_log_pause, is_log_paused, clear_logs,
            set_log_stream_active
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
mod db;
mod extension_server;
mod scheduler;
