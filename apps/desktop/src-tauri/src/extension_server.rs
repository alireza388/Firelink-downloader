use hmac::{Hmac, Mac};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const EXTENSION_SERVER_PORT: u16 = 23522;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_REQUEST_BYTES: usize = 128 * 1024;
const MAX_URL_COUNT: usize = 200;
const SIGNATURE_MAX_AGE_MS: u64 = 60_000;

type HmacSha256 = Hmac<Sha256>;
pub type SharedExtensionToken = Arc<RwLock<String>>;
pub type SharedFrontendReady = Arc<AtomicBool>;
type ReplayCache = Arc<Mutex<HashMap<String, u64>>>;

#[derive(Deserialize)]
struct ExtensionRequest {
    urls: Vec<String>,
    #[serde(default)]
    referer: Option<String>,
    #[serde(default)]
    silent: bool,
    #[serde(default)]
    filename: Option<String>,
}

#[derive(Clone, Serialize)]
struct ExtensionDownload {
    urls: Vec<String>,
    referer: Option<String>,
    silent: bool,
    filename: Option<String>,
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug)]
enum RequestError {
    BadRequest,
    LengthRequired,
    PayloadTooLarge,
}

#[derive(Clone, Copy)]
enum HttpStatus {
    Ok,
    NoContent,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    LengthRequired,
    PayloadTooLarge,
    UnsupportedMediaType,
    ServiceUnavailable,
    InternalServerError,
}

impl HttpStatus {
    fn code(self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::NoContent => 204,
            Self::BadRequest => 400,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::LengthRequired => 411,
            Self::PayloadTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::ServiceUnavailable => 503,
            Self::InternalServerError => 500,
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::NoContent => "No Content",
            Self::BadRequest => "Bad Request",
            Self::Forbidden => "Forbidden",
            Self::NotFound => "Not Found",
            Self::MethodNotAllowed => "Method Not Allowed",
            Self::LengthRequired => "Length Required",
            Self::PayloadTooLarge => "Payload Too Large",
            Self::UnsupportedMediaType => "Unsupported Media Type",
            Self::ServiceUnavailable => "Service Unavailable",
            Self::InternalServerError => "Internal Server Error",
        }
    }
}

pub fn start_server(
    app_handle: AppHandle,
    pairing_token: SharedExtensionToken,
    frontend_ready: SharedFrontendReady,
) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", EXTENSION_SERVER_PORT))
        .map_err(|error| format!("Failed to bind 127.0.0.1:{EXTENSION_SERVER_PORT}: {error}"))?;
    let replay_cache = Arc::new(Mutex::new(HashMap::new()));

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_connection(
                    stream,
                    &app_handle,
                    &pairing_token,
                    &frontend_ready,
                    &replay_cache,
                ),
                Err(error) => eprintln!("Browser extension connection failed: {error}"),
            }
        }
    });

    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    app_handle: &AppHandle,
    pairing_token: &SharedExtensionToken,
    frontend_ready: &SharedFrontendReady,
    replay_cache: &ReplayCache,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(RequestError::BadRequest) => {
            write_response(&mut stream, HttpStatus::BadRequest, None);
            return;
        }
        Err(RequestError::LengthRequired) => {
            write_response(&mut stream, HttpStatus::LengthRequired, None);
            return;
        }
        Err(RequestError::PayloadTooLarge) => {
            write_response(&mut stream, HttpStatus::PayloadTooLarge, None);
            return;
        }
    };

    let origin = request.header("origin").map(str::to_string);
    let status = process_request(
        request,
        app_handle,
        pairing_token,
        frontend_ready,
        replay_cache,
    );
    write_response(
        &mut stream,
        status,
        origin.as_deref().filter(|value| is_allowed_origin(value)),
    );
}

fn process_request(
    request: HttpRequest,
    app_handle: &AppHandle,
    pairing_token: &SharedExtensionToken,
    frontend_ready: &SharedFrontendReady,
    replay_cache: &ReplayCache,
) -> HttpStatus {
    if !is_local_host(request.header("host")) {
        return HttpStatus::Forbidden;
    }

    let origin = request.header("origin");
    if request.method == "OPTIONS" {
        return if origin.is_some_and(is_allowed_origin) {
            HttpStatus::NoContent
        } else {
            HttpStatus::Forbidden
        };
    }
    if origin.is_some_and(|value| !is_allowed_origin(value)) {
        return HttpStatus::Forbidden;
    }

    let timestamp = match verify_signature(&request, pairing_token) {
        Ok(timestamp) => timestamp,
        Err(()) => return HttpStatus::Forbidden,
    };

    if request.path == "/ping" {
        return if request.method == "GET" {
            if frontend_ready.load(Ordering::Acquire) {
                HttpStatus::Ok
            } else {
                HttpStatus::ServiceUnavailable
            }
        } else {
            HttpStatus::MethodNotAllowed
        };
    }

    if request.path != "/download" {
        return HttpStatus::NotFound;
    }
    if request.method != "POST" {
        return HttpStatus::MethodNotAllowed;
    }
    if !request
        .header("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().contains("application/json"))
    {
        return HttpStatus::UnsupportedMediaType;
    }

    let payload = match serde_json::from_slice::<ExtensionRequest>(&request.body) {
        Ok(payload) => payload,
        Err(_) => return HttpStatus::BadRequest,
    };
    let download = match normalize_download(payload) {
        Some(download) => download,
        None => return HttpStatus::BadRequest,
    };
    if !frontend_ready.load(Ordering::Acquire) {
        return HttpStatus::ServiceUnavailable;
    }

    let signature = match request.header("x-firelink-signature") {
        Some(signature) => signature,
        None => return HttpStatus::Forbidden,
    };
    if !claim_request(signature, timestamp, replay_cache) {
        return HttpStatus::Forbidden;
    }

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    if app_handle.emit("extension-add-download", download).is_err() {
        return HttpStatus::InternalServerError;
    }

    HttpStatus::Ok
}

fn normalize_download(payload: ExtensionRequest) -> Option<ExtensionDownload> {
    let mut seen = HashSet::new();
    let urls = payload
        .urls
        .into_iter()
        .take(MAX_URL_COUNT)
        .filter_map(|raw_url| normalize_url(&raw_url))
        .filter(|url| seen.insert(url.clone()))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return None;
    }

    let referer = payload.referer.and_then(|value| {
        let url = Url::parse(value.trim()).ok()?;
        matches!(url.scheme(), "http" | "https").then(|| url.to_string())
    });
    let filename = payload.filename.and_then(|value| sanitize_filename(&value));

    Some(ExtensionDownload {
        urls,
        referer,
        silent: payload.silent,
        filename,
    })
}

fn normalize_url(raw_url: &str) -> Option<String> {
    let url = Url::parse(raw_url.trim()).ok()?;
    matches!(url.scheme(), "http" | "https" | "ftp" | "sftp").then(|| url.to_string())
}

fn sanitize_filename(filename: &str) -> Option<String> {
    let normalized = filename.trim().replace('\\', "/");
    let basename = Path::new(&normalized).file_name()?.to_str()?.trim();
    if basename.is_empty() || basename == "." || basename == ".." || basename.len() > 255 {
        return None;
    }
    Some(basename.to_string())
}

fn verify_signature(
    request: &HttpRequest,
    pairing_token: &SharedExtensionToken,
) -> Result<u64, ()> {
    let signature = decode_hex(request.header("x-firelink-signature").ok_or(())?)?;
    let timestamp_text = request.header("x-firelink-timestamp").ok_or(())?;
    let timestamp = timestamp_text.parse::<u64>().map_err(|_| ())?;
    let now = current_time_millis().ok_or(())?;
    if now.abs_diff(timestamp) >= SIGNATURE_MAX_AGE_MS {
        return Err(());
    }

    let token = pairing_token.read().map_err(|_| ())?;
    if token.is_empty() {
        return Err(());
    }

    let mut mac = HmacSha256::new_from_slice(token.as_bytes()).map_err(|_| ())?;
    mac.update(timestamp_text.as_bytes());
    mac.update(&request.body);
    mac.verify_slice(&signature).map_err(|_| ())?;
    Ok(timestamp)
}

fn claim_request(signature: &str, timestamp: u64, replay_cache: &ReplayCache) -> bool {
    let now = match current_time_millis() {
        Some(now) => now,
        None => return false,
    };
    let mut cache = match replay_cache.lock() {
        Ok(cache) => cache,
        Err(_) => return false,
    };
    cache.retain(|_, seen_at| now.saturating_sub(*seen_at) < SIGNATURE_MAX_AGE_MS);
    let key = format!("{timestamp}:{}", signature.to_ascii_lowercase());
    cache.insert(key, now).is_none()
}

fn current_time_millis() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ()> {
    if value.len() != 64 || !value.is_ascii() {
        return Err(());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0]).ok_or(())?;
            let low = hex_digit(pair[1]).ok_or(())?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_local_host(host: Option<&str>) -> bool {
    matches!(
        host,
        Some(value)
            if value == format!("127.0.0.1:{EXTENSION_SERVER_PORT}")
                || value == format!("localhost:{EXTENSION_SERVER_PORT}")
                || value == "127.0.0.1"
                || value == "localhost"
    )
}

fn is_allowed_origin(origin: &str) -> bool {
    Url::parse(origin)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "moz-extension" | "chrome-extension"))
}

fn read_http_request(reader: &mut impl Read) -> Result<HttpRequest, RequestError> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut expected_length = None;

    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|_| RequestError::BadRequest)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(RequestError::PayloadTooLarge);
        }

        if expected_length.is_none() {
            if let Some(header_end) = find_bytes(&bytes, b"\r\n\r\n") {
                if header_end > MAX_HEADER_BYTES {
                    return Err(RequestError::PayloadTooLarge);
                }
                let headers = parse_headers(&bytes[..header_end])?;
                if headers.contains_key("transfer-encoding") {
                    return Err(RequestError::BadRequest);
                }
                let method = parse_request_line(&bytes[..header_end])?.0;
                let content_length = parse_content_length(&headers)?;
                if method == "POST" && content_length.is_none() {
                    return Err(RequestError::LengthRequired);
                }
                let body_length = content_length.unwrap_or(0);
                let total_length = header_end + 4 + body_length;
                if total_length > MAX_REQUEST_BYTES {
                    return Err(RequestError::PayloadTooLarge);
                }
                expected_length = Some(total_length);
            } else if bytes.len() > MAX_HEADER_BYTES {
                return Err(RequestError::PayloadTooLarge);
            }
        }

        if expected_length.is_some_and(|length| bytes.len() >= length) {
            break;
        }
    }

    let header_end = find_bytes(&bytes, b"\r\n\r\n").ok_or(RequestError::BadRequest)?;
    let (method, path) = parse_request_line(&bytes[..header_end])?;
    let headers = parse_headers(&bytes[..header_end])?;
    let body_length = parse_content_length(&headers)?.unwrap_or(0);
    let body_start = header_end + 4;
    let body_end = body_start + body_length;
    if bytes.len() < body_end {
        return Err(RequestError::BadRequest);
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[body_start..body_end].to_vec(),
    })
}

fn parse_request_line(header_bytes: &[u8]) -> Result<(String, String), RequestError> {
    let headers = std::str::from_utf8(header_bytes).map_err(|_| RequestError::BadRequest)?;
    let mut parts = headers
        .lines()
        .next()
        .ok_or(RequestError::BadRequest)?
        .split_whitespace();
    let method = parts.next().ok_or(RequestError::BadRequest)?;
    let raw_path = parts.next().ok_or(RequestError::BadRequest)?;
    let version = parts.next().ok_or(RequestError::BadRequest)?;
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(RequestError::BadRequest);
    }
    let path = raw_path.split('?').next().unwrap_or_default();
    Ok((method.to_ascii_uppercase(), path.to_string()))
}

fn parse_headers(header_bytes: &[u8]) -> Result<HashMap<String, String>, RequestError> {
    let headers = std::str::from_utf8(header_bytes).map_err(|_| RequestError::BadRequest)?;
    let mut parsed = HashMap::new();
    for line in headers.lines().skip(1) {
        let (name, value) = line.split_once(':').ok_or(RequestError::BadRequest)?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || parsed.contains_key(&name) {
            return Err(RequestError::BadRequest);
        }
        parsed.insert(name, value.trim().to_string());
    }
    Ok(parsed)
}

fn parse_content_length(
    headers: &HashMap<String, String>,
) -> Result<Option<usize>, RequestError> {
    headers
        .get("content-length")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| RequestError::BadRequest)
        })
        .transpose()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_response(stream: &mut impl Write, status: HttpStatus, origin: Option<&str>) {
    let mut headers = vec![
        format!("HTTP/1.1 {} {}", status.code(), status.reason()),
        "Content-Length: 0".to_string(),
        "Connection: close".to_string(),
        "X-Firelink-Server: 1".to_string(),
    ];
    if let Some(origin) = origin {
        headers.push(format!("Access-Control-Allow-Origin: {origin}"));
        headers.push("Vary: Origin".to_string());
        headers.push("Access-Control-Allow-Methods: GET, POST, OPTIONS".to_string());
        headers.push(
            "Access-Control-Allow-Headers: Content-Type, X-Firelink-Signature, X-Firelink-Timestamp"
                .to_string(),
        );
        headers.push("Access-Control-Allow-Private-Network: true".to_string());
        headers.push("Access-Control-Expose-Headers: X-Firelink-Server".to_string());
    }
    let response = headers.join("\r\n") + "\r\n\r\n";
    let _ = stream.write_all(response.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct ChunkedReader {
        bytes: Cursor<Vec<u8>>,
        chunk_size: usize,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let limit = buffer.len().min(self.chunk_size);
            self.bytes.read(&mut buffer[..limit])
        }
    }

    fn token(value: &str) -> SharedExtensionToken {
        Arc::new(RwLock::new(value.to_string()))
    }

    fn sign(value: &str, timestamp: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(value.as_bytes()).unwrap();
        mac.update(timestamp.as_bytes());
        mac.update(body);
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn reads_fragmented_signed_request() {
        let body = br#"{"urls":["https://example.com/file.zip"],"silent":true}"#;
        let request = format!(
            "POST /download HTTP/1.1\r\nHost: 127.0.0.1:6412\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        );
        let mut reader = ChunkedReader {
            bytes: Cursor::new(request.into_bytes()),
            chunk_size: 7,
        };

        let parsed = read_http_request(&mut reader).expect("request should parse");

        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.path, "/download");
        assert_eq!(parsed.body, body);
    }

    #[test]
    fn verifies_hmac_over_timestamp_and_exact_body() {
        let body = br#"{"urls":["https://example.com/file.zip"]}"#.to_vec();
        let timestamp = current_time_millis().unwrap().to_string();
        let signature = sign("secret", &timestamp, &body);
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/download".to_string(),
            headers: HashMap::from([
                ("x-firelink-signature".to_string(), signature),
                ("x-firelink-timestamp".to_string(), timestamp),
            ]),
            body,
        };

        assert!(verify_signature(&request, &token("secret")).is_ok());
        assert!(verify_signature(&request, &token("wrong")).is_err());
    }

    #[test]
    fn rejects_replayed_download_signature() {
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let timestamp = current_time_millis().unwrap();

        assert!(claim_request("signature", timestamp, &cache));
        assert!(!claim_request("signature", timestamp, &cache));
    }

    #[test]
    fn normalizes_payload_and_sanitizes_filename() {
        let download = normalize_download(ExtensionRequest {
            urls: vec![
                "https://example.com/file.zip".to_string(),
                "https://example.com/file.zip".to_string(),
                "javascript:alert(1)".to_string(),
            ],
            referer: Some("https://example.com/page".to_string()),
            silent: true,
            filename: Some("../archive.zip".to_string()),
        })
        .unwrap();

        assert_eq!(download.urls, vec!["https://example.com/file.zip"]);
        assert_eq!(
            download.referer.as_deref(),
            Some("https://example.com/page")
        );
        assert_eq!(download.filename.as_deref(), Some("archive.zip"));
    }

    #[test]
    fn requires_content_length_for_post() {
        let mut reader =
            Cursor::new(b"POST /download HTTP/1.1\r\nHost: localhost\r\n\r\n{}".to_vec());

        assert!(matches!(
            read_http_request(&mut reader),
            Err(RequestError::LengthRequired)
        ));
    }
}
