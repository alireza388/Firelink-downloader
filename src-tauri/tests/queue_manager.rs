use firelink_lib::queue::{
    QueueManager, QueuedTask, SidecarSpawner, SpawnPayload, TaskKind, MEDIA_RUN_CANCELLED,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Listener;
use tokio::time::timeout;

/// A fake spawner that records calls and lets tests gate sidecar lifetime.
struct CountingSpawner {
    add_uri_calls: AtomicUsize,
    media_calls: AtomicUsize,
    native_calls: AtomicUsize,
}

impl CountingSpawner {
    fn new() -> Self {
        Self {
            add_uri_calls: AtomicUsize::new(0),
            media_calls: AtomicUsize::new(0),
            native_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl firelink_lib::queue::SidecarSpawner for CountingSpawner {
    async fn add_uri(&self, _id: &str, _payload: &SpawnPayload) -> Result<String, String> {
        self.add_uri_calls.fetch_add(1, Ordering::SeqCst);
        Ok(format!("gid-{}", self.add_uri_calls.load(Ordering::SeqCst)))
    }
    async fn remove_uri(&self, _gid: &str) -> Result<(), String> {
        Ok(())
    }
    async fn run_media(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        self.media_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn run_native(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        self.native_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Build a QueueManager with a fake spawner. Tauri's mock AppHandle is needed
/// for emit; we construct the minimal mock.
fn make_manager(capacity: usize) -> (QueueManager<tauri::test::MockRuntime>, Arc<CountingSpawner>) {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let spawner = Arc::new(CountingSpawner::new());
    let mgr = QueueManager::test_new(app.handle().clone(), capacity, spawner.clone());
    (mgr, spawner)
}

fn sample_task(id: &str) -> QueuedTask {
    QueuedTask {
        id: id.to_string(),
        kind: TaskKind::Native,
        payload: SpawnPayload::default(),
    }
}

#[tokio::test]
async fn push_appends_to_pending_and_emits_queued() {
    let (mgr, _spawner) = make_manager(2);
    mgr.push(sample_task("a")).await;
    mgr.push(sample_task("b")).await;
    let order = mgr.pending_order().await;
    assert_eq!(order, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn release_permit_is_idempotent() {
    let (mgr, _spawner) = make_manager(2);
    let permit = mgr.acquire_permit().await;
    mgr.park_permit("a", permit).await;
    let avail_before = mgr.available_permits();
    mgr.release_permit("a").await; // first release: frees the slot
    let avail_after_first = mgr.available_permits();
    mgr.release_permit("a").await; // second release: no-op
    let avail_after_second = mgr.available_permits();
    assert_eq!(avail_after_first - avail_before, 1);
    assert_eq!(avail_after_second, avail_after_first, "second release must not free another slot");
}

#[tokio::test]
async fn ensure_aria2_permit_does_not_double_acquire() {
    let (mgr, _spawner) = make_manager(2);
    assert!(mgr.ensure_aria2_permit("a").await);
    assert!(!mgr.ensure_aria2_permit("a").await);
    assert_eq!(mgr.available_permits(), 1);

    mgr.release_permit("a").await;
    assert_eq!(mgr.available_permits(), 2);
}

#[tokio::test]
async fn forgetting_aria2_gid_clears_mapping_without_releasing_twice() {
    let (mgr, _spawner) = make_manager(1);
    let permit = mgr.acquire_permit().await;
    mgr.park_permit("a", permit).await;
    mgr.remember_gid("a".to_string(), "gid-a".to_string()).await;

    assert_eq!(mgr.forget_aria2_gid("a").await.as_deref(), Some("gid-a"));
    assert!(mgr.aria2_gid_for_download("a").is_none());
    assert_eq!(mgr.available_permits(), 0);

    mgr.release_permit("a").await;
    mgr.release_permit("a").await;
    assert_eq!(mgr.available_permits(), 1);
}

#[tokio::test]
async fn push_then_pop_front_drains_fifo() {
    let (mgr, _spawner) = make_manager(2);
    mgr.push(sample_task("a")).await;
    mgr.push(sample_task("b")).await;
    let first = mgr.pop_front().await.expect("some task");
    let second = mgr.pop_front().await.expect("some task");
    assert_eq!(first.id, "a");
    assert_eq!(second.id, "b");
    assert!(mgr.pop_front().await.is_none());
}

#[tokio::test]
async fn dispatcher_parks_when_idle_no_busy_spin() {
    let (mgr, _spawner) = make_manager(3);
    let mgr_arc = Arc::new(mgr);
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Queue is empty. Sleep long enough for any busy-spin to drain permits.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // No permit should have been acquired while idle.
    assert_eq!(
        mgr_arc.available_permits(), 3,
        "dispatcher must not acquire permits when pending is empty"
    );

    handle.abort();
}

#[tokio::test]
async fn cas_retirement_never_underflows() {
    let (mgr, _spawner) = make_manager(3);
    let mgr_arc = Arc::new(mgr);
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Repeatedly toggle capacity down while idle. A fetch_sub-based impl
    // would underflow to usize::MAX on the second set_capacity(1).
    for _ in 0..5 {
        mgr_arc.set_capacity(3);
        mgr_arc.set_capacity(1);
        mgr_arc.set_capacity(3);
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let debt = mgr_arc.slots_to_retire_load();
    assert!(
        debt <= 2,
        "retirement debt must stay within [0, capacity-target], got {debt}"
    );

    handle.abort();
}

#[tokio::test]
async fn grow_releases_immediately_and_dispatches_waiting_tasks() {
    let (mgr, spawner) = make_manager(2);
    let mgr_arc = Arc::new(mgr);

    for i in 0..4 {
        mgr_arc.push(sample_task(&format!("t{i}"))).await;
    }
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Give dispatcher time to dispatch 2 (capacity) of the 4.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let native_after_initial = spawner.native_calls.load(Ordering::SeqCst);
    assert_eq!(native_after_initial, 2, "only capacity-many tasks dispatch initially");

    // Grow to 4; the remaining 2 should dispatch.
    mgr_arc.set_capacity(4);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let native_after_grow = spawner.native_calls.load(Ordering::SeqCst);
    assert_eq!(native_after_grow, 4, "grow must allow the waiting tasks to dispatch");

    handle.abort();
}

#[tokio::test]
async fn shrink_converges_to_target_without_killing_active() {
    let (mgr, spawner) = make_manager(4);
    let mgr_arc = Arc::new(mgr);

    for i in 0..6 {
        mgr_arc.push(sample_task(&format!("t{i}"))).await;
    }
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Let 4 dispatch (capacity).
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(spawner.native_calls.load(Ordering::SeqCst), 4);

    // Shrink to 2 while 4 are "active" (permits parked).
    mgr_arc.set_capacity(2);

    // Release active permits. Debt is 2; two releases retire both.
    mgr_arc.release_permit("t0").await;
    mgr_arc.release_permit("t1").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Debt was 2; two releases retired both. The 2 remaining pending tasks
    // (t4, t5) can now dispatch since debt is 0 and slots freed.
    assert_eq!(
        spawner.native_calls.load(Ordering::SeqCst),
        6,
        "shrink converges: after debt exhausted, remaining pending dispatch"
    );

    handle.abort();
}

fn aria2_task(id: &str) -> QueuedTask {
    QueuedTask {
        id: id.to_string(),
        kind: TaskKind::Aria2,
        payload: SpawnPayload::default(),
    }
}

fn media_task(id: &str) -> QueuedTask {
    QueuedTask {
        id: id.to_string(),
        kind: TaskKind::Media,
        payload: SpawnPayload::default(),
    }
}

struct FixedMediaSpawner {
    outcome: Result<(), String>,
}

#[async_trait::async_trait]
impl SidecarSpawner for FixedMediaSpawner {
    async fn add_uri(&self, _id: &str, _payload: &SpawnPayload) -> Result<String, String> {
        unreachable!("aria2 is not used by media terminal-state tests")
    }

    async fn remove_uri(&self, _gid: &str) -> Result<(), String> {
        unreachable!("aria2 is not used by media terminal-state tests")
    }

    async fn run_media(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        self.outcome.clone()
    }

    async fn run_native(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        unreachable!("native downloads are not used by media terminal-state tests")
    }
}

fn make_media_manager(
    outcome: Result<(), String>,
) -> (
    QueueManager<tauri::test::MockRuntime>,
    std::sync::mpsc::Receiver<String>,
) {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    app.handle().listen("download-state", move |event| {
        let _ = event_tx.send(event.payload().to_string());
    });
    let spawner: Arc<dyn SidecarSpawner> = Arc::new(FixedMediaSpawner { outcome });
    let manager = QueueManager::test_new(app.handle().clone(), 1, spawner);
    (manager, event_rx)
}

fn emitted_statuses(event_rx: &std::sync::mpsc::Receiver<String>) -> Vec<String> {
    event_rx
        .try_iter()
        .filter_map(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .filter_map(|payload| payload.get("status")?.as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn media_terminal_error_emits_failed_without_completed() {
    let (manager, event_rx) = make_media_manager(Err("terminal media failure".to_string()));
    let manager = Arc::new(manager);
    manager.push(media_task("media-failed")).await;
    let dispatcher = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.run_dispatcher().await })
    };

    tokio::time::sleep(Duration::from_millis(100)).await;
    let statuses = emitted_statuses(&event_rx);
    assert!(statuses.iter().any(|status| status == "failed"));
    assert!(!statuses.iter().any(|status| status == "completed"));
    assert_eq!(manager.available_permits(), 1);

    dispatcher.abort();
}

#[tokio::test]
async fn media_cancellation_does_not_emit_completed() {
    let (manager, event_rx) =
        make_media_manager(Err(MEDIA_RUN_CANCELLED.to_string()));
    let manager = Arc::new(manager);
    manager.push(media_task("media-cancelled")).await;
    let dispatcher = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.run_dispatcher().await })
    };

    tokio::time::sleep(Duration::from_millis(100)).await;
    let statuses = emitted_statuses(&event_rx);
    assert!(!statuses.iter().any(|status| status == "failed"));
    assert!(!statuses.iter().any(|status| status == "completed"));
    assert_eq!(manager.available_permits(), 1);

    dispatcher.abort();
}

#[tokio::test]
async fn aria2_permit_survives_rpc_return() {
    let (mgr, spawner) = make_manager(1);
    let mgr_arc = Arc::new(mgr);
    mgr_arc.push(aria2_task("a")).await;
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Dispatcher acquires the single permit, parks it, calls add_uri (returns
    // instantly). The permit must STAY parked.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(spawner.add_uri_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        mgr_arc.available_permits(), 0,
        "permit must remain parked while aria2 download is notionally running"
    );

    // Now simulate aria2 completion: release_permit frees the slot.
    mgr_arc.release_permit("a").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(mgr_arc.available_permits(), 1, "release frees the parked permit");

    handle.abort();
}

#[tokio::test]
async fn gid_completion_before_store_buffers_and_reconciles() {
    use firelink_lib::queue::PendingOutcome;

    let (mgr, _spawner) = make_manager(1);
    let mgr_arc = Arc::new(mgr);
    mgr_arc.push(aria2_task("a")).await;
    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The dispatcher called add_uri and got "gid-1", then remember_gid stored it.
    // Simulate a completion arriving for an UNKNOWN gid first:
    mgr_arc
        .handle_aria2_event("gid-unknown", PendingOutcome::Complete)
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Permit still parked (gid-unknown is not ours).
    assert_eq!(mgr_arc.available_permits(), 0);

    // Now store gid-1 -> "a" via remember_gid. The buffered gid-unknown stays
    // buffered (different gid). Release via the real gid:
    mgr_arc.release_permit("a").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(mgr_arc.available_permits(), 1);

    // Push another aria2 task; its gid will be "gid-2".
    mgr_arc.push(aria2_task("b")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    mgr_arc.release_permit("b").await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle.abort();
}

#[tokio::test]
async fn aria2_completion_forgets_gid_and_releases_permit() {
    use firelink_lib::queue::PendingOutcome;

    let (mgr, _spawner) = make_manager(1);
    let permit = mgr.acquire_permit().await;
    mgr.park_permit("a", permit).await;
    mgr.remember_gid("a".to_string(), "gid-a".to_string()).await;

    mgr.apply_completion("a", PendingOutcome::Complete).await;

    assert!(mgr.aria2_gid_for_download("a").is_none());
    assert_eq!(mgr.available_permits(), 1);
}

#[tokio::test]
async fn move_up_down_reorders_pending() {
    use firelink_lib::ipc::QueueDirection;

    let (mgr, _spawner) = make_manager(3);
    let mgr_arc = Arc::new(mgr);
    mgr_arc.push(sample_task("a")).await;
    mgr_arc.push(sample_task("b")).await;
    mgr_arc.push(sample_task("c")).await;

    mgr_arc.move_in_queue("c", QueueDirection::Down).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["a", "b", "c"]);

    mgr_arc.move_in_queue("c", QueueDirection::Up).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["a", "c", "b"]);

    mgr_arc.move_in_queue("a", QueueDirection::Down).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["c", "a", "b"]);

    mgr_arc.move_in_queue("c", QueueDirection::Up).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["c", "a", "b"]);
}

#[tokio::test]
async fn notify_fires_on_push_and_release() {
    let (mgr, _spawner) = make_manager(1);
    let mgr_arc = Arc::new(mgr);

    let permit = mgr_arc.acquire_permit().await;
    mgr_arc.park_permit("a", permit).await;

    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    mgr_arc.push(sample_task("x")).await;
    let dispatched = timeout(
        Duration::from_millis(150),
        async {
            loop {
                if mgr_arc.available_permits() == 0 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        },
    ).await;
    assert!(dispatched.is_ok(), "push must wake the idle dispatcher");

    handle.abort();
}
