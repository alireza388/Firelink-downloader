use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{
        header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::Response,
    routing::get,
    Router,
};
use firelink_lib::download::{DownloadCmd, DownloadCoordinator, DownloadEvent, DownloadPayload};
use futures_util::stream;
use sha2::{Digest, Sha256};
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct ServerState {
    data: Arc<Vec<u8>>,
    chunk_size: usize,
    chunk_delay: Duration,
    failures_remaining: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    chunks_served: Arc<AtomicUsize>,
    range_starts: Arc<Mutex<Vec<Option<usize>>>>,
}

struct TestServer {
    base_url: String,
    state: ServerState,
    task: JoinHandle<()>,
}

impl TestServer {
    async fn spawn(
        data: Vec<u8>,
        chunk_size: usize,
        chunk_delay: Duration,
        failures_before_success: usize,
    ) -> Self {
        let state = ServerState {
            data: Arc::new(data),
            chunk_size,
            chunk_delay,
            failures_remaining: Arc::new(AtomicUsize::new(failures_before_success)),
            requests: Arc::new(AtomicUsize::new(0)),
            chunks_served: Arc::new(AtomicUsize::new(0)),
            range_starts: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/file", get(serve_file))
            .route("/always-fail", get(always_fail))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            base_url: format!("http://{address}"),
            state,
            task,
        }
    }

    fn file_url(&self) -> String {
        format!("{}/file", self.base_url)
    }

    fn failure_url(&self) -> String {
        format!("{}/always-fail", self.base_url)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve_file(State(state): State<ServerState>, headers: HeaderMap) -> Response<Body> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    if state
        .failures_remaining
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
            remaining.checked_sub(1)
        })
        .is_ok()
    {
        return response(StatusCode::SERVICE_UNAVAILABLE, Body::empty(), 0, None);
    }

    let requested_start = headers
        .get(RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_range_start);
    state.range_starts.lock().unwrap().push(requested_start);

    let start = requested_start.unwrap_or(0);
    if start >= state.data.len() {
        return response(
            StatusCode::RANGE_NOT_SATISFIABLE,
            Body::empty(),
            0,
            Some(format!("bytes */{}", state.data.len())),
        );
    }

    let status = if requested_start.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let content_range = requested_start.map(|_| {
        format!(
            "bytes {start}-{}/{}",
            state.data.len() - 1,
            state.data.len()
        )
    });
    let content_length = state.data.len() - start;
    let data = state.data.clone();
    let chunk_size = state.chunk_size;
    let chunk_delay = state.chunk_delay;
    let chunks_served = state.chunks_served.clone();
    let body_stream = stream::unfold(start, move |offset| {
        let data = data.clone();
        let chunks_served = chunks_served.clone();
        async move {
            if offset >= data.len() {
                return None;
            }
            if !chunk_delay.is_zero() {
                tokio::time::sleep(chunk_delay).await;
            }
            let end = (offset + chunk_size).min(data.len());
            chunks_served.fetch_add(1, Ordering::SeqCst);
            let chunk = Bytes::copy_from_slice(&data[offset..end]);
            Some((Ok::<_, Infallible>(chunk), end))
        }
    });

    response(
        status,
        Body::from_stream(body_stream),
        content_length,
        content_range,
    )
}

async fn always_fail(State(state): State<ServerState>) -> Response<Body> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    response(StatusCode::INTERNAL_SERVER_ERROR, Body::empty(), 0, None)
}

fn response(
    status: StatusCode,
    body: Body,
    content_length: usize,
    content_range: Option<String>,
) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string()).unwrap(),
    );
    if let Some(content_range) = content_range {
        response.headers_mut().insert(
            CONTENT_RANGE,
            HeaderValue::from_str(&content_range).unwrap(),
        );
    }
    response
}

fn parse_range_start(value: &str) -> Option<usize> {
    value
        .strip_prefix("bytes=")?
        .strip_suffix('-')?
        .parse()
        .ok()
}

fn fixture_data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| ((index.wrapping_mul(31) + index / 7) % 251) as u8)
        .collect()
}

fn payload(id: Uuid, url: String, output_path: std::path::PathBuf) -> DownloadPayload {
    DownloadPayload {
        id,
        urls: vec![url],
        output_path,
        speed_limit: None,
        username: None,
        password: None,
        headers: None,
        cookies: None,
        user_agent: Some("Firelink integration test".to_string()),
        max_tries: 1,
        proxy: None,
    }
}

async fn next_event(events: &mut mpsc::UnboundedReceiver<DownloadEvent>) -> DownloadEvent {
    tokio::time::timeout(TEST_TIMEOUT, events.recv())
        .await
        .expect("timed out waiting for download event")
        .expect("download event channel closed")
}

async fn wait_for_progress(
    events: &mut mpsc::UnboundedReceiver<DownloadEvent>,
    id: Uuid,
    minimum_completed: u64,
) {
    loop {
        match next_event(events).await {
            DownloadEvent::Progress {
                id: event_id,
                completed,
                ..
            } if event_id == id && completed >= minimum_completed => return,
            DownloadEvent::Failed { error, .. } => panic!("download failed: {error}"),
            DownloadEvent::Completed(event_id) if event_id == id => {
                panic!("download completed before it could be paused")
            }
            _ => {}
        }
    }
}

async fn wait_for_completion(
    events: &mut mpsc::UnboundedReceiver<DownloadEvent>,
    id: Uuid,
) -> Vec<DownloadEvent> {
    let mut observed = Vec::new();
    loop {
        let event = next_event(events).await;
        match &event {
            DownloadEvent::Completed(event_id) if *event_id == id => {
                observed.push(event);
                return observed;
            }
            DownloadEvent::Failed {
                id: event_id,
                error,
            } if *event_id == id => panic!("download failed: {error}"),
            _ => observed.push(event),
        }
    }
}

fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_then_resume_uses_range_and_preserves_integrity() {
    let expected = fixture_data(4 * 1024 * 1024);
    let server = TestServer::spawn(expected.clone(), 16 * 1024, Duration::from_millis(4), 0).await;
    let temp = TempDir::new().unwrap();
    let output_path = temp.path().join("paused.bin");
    let id = Uuid::from_u128(1);
    let download = payload(id, server.file_url(), output_path.clone());
    let (coordinator, mut events) = DownloadCoordinator::spawn_headless();

    coordinator
        .send(DownloadCmd::Start(Box::new(download)))
        .await
        .unwrap();
    wait_for_progress(&mut events, id, 256 * 1024).await;
    let (pause_tx, pause_rx) = oneshot::channel();
    coordinator
        .send(DownloadCmd::PauseWithAck(id, pause_tx))
        .await
        .unwrap();
    pause_rx.await.unwrap();

    let paused_len = tokio::fs::metadata(&output_path).await.unwrap().len();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stable_paused_len = tokio::fs::metadata(&output_path).await.unwrap().len();
    assert!(paused_len >= 256 * 1024);
    assert!(paused_len < expected.len() as u64);
    assert_eq!(stable_paused_len, paused_len);

    coordinator
        .send(DownloadCmd::Start(Box::new(payload(
            id,
            server.file_url(),
            output_path.clone(),
        ))))
        .await
        .unwrap();
    wait_for_completion(&mut events, id).await;

    let downloaded = tokio::fs::read(&output_path).await.unwrap();
    assert_eq!(downloaded.len(), expected.len());
    assert_eq!(sha256(&downloaded), sha256(&expected));
    let range_starts = server.state.range_starts.lock().unwrap().clone();
    assert_eq!(range_starts.first(), Some(&None));
    assert!(
        range_starts
            .iter()
            .skip(1)
            .flatten()
            .any(|start| *start == paused_len as usize),
        "resume request did not start at the paused file length: {range_starts:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn aggregates_many_http_chunks_with_complete_progress() {
    let expected = fixture_data(3 * 1024 * 1024);
    let server = TestServer::spawn(expected.clone(), 8 * 1024, Duration::ZERO, 0).await;
    let temp = TempDir::new().unwrap();
    let output_path = temp.path().join("chunked.bin");
    let id = Uuid::from_u128(2);
    let (coordinator, mut events) = DownloadCoordinator::spawn_headless();
    let started = Instant::now();

    coordinator
        .send(DownloadCmd::Start(Box::new(payload(
            id,
            server.file_url(),
            output_path.clone(),
        ))))
        .await
        .unwrap();
    let observed = wait_for_completion(&mut events, id).await;

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "local 3 MiB transfer exceeded the performance budget"
    );
    assert!(server.state.chunks_served.load(Ordering::SeqCst) > 100);
    let final_progress = observed.iter().rev().find_map(|event| match event {
        DownloadEvent::Progress {
            id: event_id,
            fraction,
            completed,
            total,
        } if *event_id == id => Some((*fraction, *completed, *total)),
        _ => None,
    });
    assert_eq!(
        final_progress,
        Some((1.0, expected.len() as u64, Some(expected.len() as u64)))
    );
    assert_eq!(
        sha256(&tokio::fs::read(output_path).await.unwrap()),
        sha256(&expected)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retries_transient_http_failures_then_completes() {
    let expected = fixture_data(512 * 1024);
    let server = TestServer::spawn(expected.clone(), 32 * 1024, Duration::ZERO, 2).await;
    let temp = TempDir::new().unwrap();
    let output_path = temp.path().join("retry.bin");
    let id = Uuid::from_u128(3);
    let (coordinator, mut events) = DownloadCoordinator::spawn_headless();
    let mut download = payload(id, server.file_url(), output_path.clone());
    download.max_tries = 3;

    coordinator
        .send(DownloadCmd::Start(Box::new(download)))
        .await
        .unwrap();
    wait_for_completion(&mut events, id).await;

    assert_eq!(server.state.requests.load(Ordering::SeqCst), 3);
    assert_eq!(
        sha256(&tokio::fs::read(output_path).await.unwrap()),
        sha256(&expected)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reports_terminal_http_errors_after_retry_budget() {
    let server = TestServer::spawn(Vec::new(), 8 * 1024, Duration::ZERO, 0).await;
    let temp = TempDir::new().unwrap();
    let output_path = temp.path().join("failed.bin");
    let id = Uuid::from_u128(4);
    let (coordinator, mut events) = DownloadCoordinator::spawn_headless();
    let mut download = payload(id, server.failure_url(), output_path.clone());
    download.max_tries = 2;

    coordinator
        .send(DownloadCmd::Start(Box::new(download)))
        .await
        .unwrap();
    let error = loop {
        match next_event(&mut events).await {
            DownloadEvent::Failed {
                id: event_id,
                error,
            } if event_id == id => break error,
            DownloadEvent::Completed(event_id) if event_id == id => {
                panic!("failed download was reported as complete")
            }
            _ => {}
        }
    };

    assert!(error.contains("500 Internal Server Error"));
    assert_eq!(server.state.requests.load(Ordering::SeqCst), 3);
    assert!(!output_path.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_removes_partial_file_without_terminal_success_event() {
    let expected = fixture_data(4 * 1024 * 1024);
    let server = TestServer::spawn(expected, 16 * 1024, Duration::from_millis(4), 0).await;
    let temp = TempDir::new().unwrap();
    let output_path = temp.path().join("cancelled.bin");
    let id = Uuid::from_u128(5);
    let (coordinator, mut events) = DownloadCoordinator::spawn_headless();

    coordinator
        .send(DownloadCmd::Start(Box::new(payload(
            id,
            server.file_url(),
            output_path.clone(),
        ))))
        .await
        .unwrap();
    wait_for_progress(&mut events, id, 256 * 1024).await;
    let (cancelled_tx, cancelled_rx) = tokio::sync::oneshot::channel();
    coordinator
        .send(DownloadCmd::CancelWithAck(id, cancelled_tx))
        .await
        .unwrap();
    cancelled_rx.await.unwrap();

    tokio::time::timeout(TEST_TIMEOUT, async {
        while output_path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled partial file was not removed");

    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_millis(100), events.recv()).await
    {
        assert!(
            !matches!(
                event,
                DownloadEvent::Completed(event_id) if event_id == id
            ),
            "cancelled download emitted a completion event"
        );
        assert!(
            !matches!(
                event,
                DownloadEvent::Failed { id: event_id, .. } if event_id == id
            ),
            "cancelled download emitted a failure event"
        );
    }
}
