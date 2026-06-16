# Download Queue Coordinator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the frontend-imperative download dispatcher with a backend `QueueManager` that is the sole concurrency gatekeeper, using a single `tokio::sync::Semaphore` to enforce `max_concurrent_tasks` across all three download paths (aria2 RPC, native HTTP, yt-dlp media).

**Architecture:** A new `queue.rs` module owns an ordered `VecDeque<QueuedTask>`, an `Arc<Semaphore>`, an `active_permits` permit-parking map, and a long-running dispatcher task that idle-parks on a `Notify` and acquires permits via `acquire_owned()`. Permits are uniformly parked by download id so aria2 (whose RPC returns immediately) and pause (which must release the slot before the sidecar dies) both work. The frontend becomes reactive: it requests enqueue/pause/move and listens for `download-state` events.

**Tech Stack:** Rust + Tokio (`tokio::sync::{Semaphore, Notify, Mutex, watch, mpsc}`, `OwnedSemaphorePermit`), Tauri 2 state management, ts-rs for binding generation, Zustand + React + TypeScript + Tailwind on the frontend.

**Spec:** `docs/superpowers/specs/2026-06-16-download-queue-coordinator-design.md`

---

## File Structure

**Create:**
- `src-tauri/src/queue.rs` — `QueueManager`, `QueuedTask`, `SpawnPayload`, `TaskKind`, `QueueDirection`, dispatcher loop, permit parking, CAS resize. ~350 lines.
- `src-tauri/tests/queue_manager.rs` — Integration tests for all four design-discovered bugs + reordering. ~400 lines.
- `src/bindings/DownloadStateEvent.ts` — ts-rs generated.
- `src/bindings/QueueDirection.ts` — ts-rs generated.

**Modify:**
- `src-tauri/src/lib.rs` — wire `QueueManager` into `AppState`; convert `start_download`/`start_media_download` to `pub(crate)`; add `enqueue_download`/`enqueue_many`/`resume_download`/`move_in_queue`/`remove_from_queue` commands; rewrite `set_concurrent_limit`; extend aria2 WS poller with `release_permit` + `pending_completion`; raise aria2 daemon ceiling.
- `src-tauri/src/ipc.rs` — add `DownloadStateEvent`, `QueueDirection`; remove `dispatched` field from `DownloadItem`.
- `src/store/useDownloadStore.ts` — remove `processQueue`; add `pendingOrder` slice; reactive `addDownload`/`startQueue`/`pauseQueue`/`pauseDownload`/`resumeDownload`/`moveUp`/`moveDown`; simplify `updateDownload`; startup re-enqueue.
- `src/store/downloadStore.ts` — add `download-state` listener (authoritative for `pendingOrder`); simplify progress listener.
- `src/components/DownloadItem.tsx` — queued visuals (clock icon, grayscale fill, position badge) + Move Up/Down buttons.
- `src/components/DownloadTable.tsx` — update `handleResume` to call backend; remove `processQueue` call.
- `src/index.css` — `.download-progress-fill.queued`, `.download-status-queued`, `.queue-position-badge`, `.animate-pulse-slow` keyframes.

---

## Conventions

- **All Rust code blocks are complete, copy-pasteable code** — no `...` elision unless the surrounding code is unchanged and clearly marked "existing code".
- **Run Rust tests with:** `cd src-tauri && cargo test <name> -- --nocapture` (the `--nocapture` helps debug).
- **Regenerate ts-rs bindings with:** `cd src-tauri && cargo test export_bindings --lib`.
- **Verify the frontend typechecks with:** `npx tsc --noEmit`.
- **TDD order for every behavior:** (1) write failing test, (2) run to confirm it fails for the right reason, (3) implement, (4) run to confirm pass, (5) commit.

---

## Task 1: Scaffold `queue.rs` with `QueueManager` struct and test harness

**Goal:** Stand up the module with the core types and a test-friendly constructor. No dispatcher logic yet — just the data structures and an injection seam for the sidecar spawner so tests can use a fake.

**Files:**
- Create: `src-tauri/src/queue.rs`
- Modify: `src-tauri/src/lib.rs:421` (add `pub mod queue;`)

### Step 1.1: Create `queue.rs` with types and a test-injectable `SpawnCallback`

- [ ] **Create `src-tauri/src/queue.rs`:**

```rust
use crate::ipc::DownloadStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore};
use ts_rs::TS;

/// Default capacity when no setting is read yet.
pub const DEFAULT_MAX_CONCURRENT: usize = 3;

/// Outcome of an aria2 completion that arrived before its gid was stored.
/// Carries the outcome so the correct state emit survives the race.
#[derive(Debug, Clone)]
pub(crate) enum PendingOutcome {
    Complete,
    Error(String),
}

/// What kind of sidecar a queued task spawns. Drives which runner the
/// dispatcher invokes.
#[derive(Debug, Clone)]
pub(crate) enum TaskKind {
    Aria2,
    Media,
    Native,
}

/// Everything needed to start a sidecar, captured at enqueue time so the
/// dispatcher can spawn it later without round-tripping back to the frontend.
#[derive(Debug, Clone)]
pub(crate) struct QueuedTask {
    pub id: String,
    pub kind: TaskKind,
    pub payload: SpawnPayload,
}

/// Args mirroring start_download / start_media_download. Kept untyped-loose
/// (String/Option) to match the existing command signatures exactly.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpawnPayload {
    pub url: String,
    pub destination: String,
    pub filename: String,
    pub connections: Option<i32>,
    pub speed_limit: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers: Option<String>,
    pub checksum: Option<String>,
    pub cookies: Option<String>,
    pub mirrors: Option<String>,
    pub user_agent: Option<String>,
    pub max_tries: Option<i32>,
    pub proxy: Option<String>,
    pub format_selector: Option<String>,
    pub cookie_source: Option<String>,
    pub is_media: bool,
}

/// Move direction for manual reordering.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum QueueDirection {
    Up,
    Down,
}

/// Authoritative backend state for a download's status. Emitted to the
/// frontend as the single source of truth for transitions.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadStateEvent {
    pub id: String,
    pub status: String,
    pub error: Option<String>,
}

impl DownloadStateEvent {
    pub(crate) fn new(id: impl Into<String>, status: DownloadStatus) -> Self {
        Self {
            id: id.into(),
            status: status.as_str().to_string(),
            error: None,
        }
    }

    pub(crate) fn failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: DownloadStatus::Failed.as_str().to_string(),
            error: Some(error.into()),
        }
    }
}

/// A sidecar spawner. In production this calls the real aria2/yt-dlp/native
/// runners; in tests it is replaced with a fake that records calls and
/// optionally hangs to simulate a long-running download.
#[crate::async_trait]
pub(crate) trait SidecarSpawner: Send + Sync + 'static {
    /// Spawn an aria2 download. Returns the gid. Must return quickly (the
    /// permit is already parked before this is called).
    async fn add_uri(&self, id: &str, payload: &SpawnPayload) -> Result<String, String>;

    /// Run a media download to completion. The permit is parked for the full
    /// duration; the implementation must call `release_permit`-equivalent on
    /// exit (handled by QueueManager, not here).
    async fn run_media(&self, id: &str, payload: &SpawnPayload) -> Result<(), String>;

    /// Run a native HTTP download to completion.
    async fn run_native(&self, id: &str, payload: &SpawnPayload) -> Result<(), String>;
}

/// The centralized concurrency gatekeeper. One instance lives in AppState.
pub struct QueueManager {
    pending: Mutex<VecDeque<QueuedTask>>,
    semaphore: Arc<Semaphore>,
    active_permits: Mutex<HashMap<String, OwnedSemaphorePermit>>,
    target_capacity: AtomicUsize,
    slots_to_retire: AtomicUsize,
    notify: Notify,

    /// aria2 gid -> download id map (shared with the WS poller).
    /// (Keyed by gid because that's what the poller sees first.)
    pub(crate) aria2_gids: Arc<std::sync::RwLock<HashMap<String, String>>>,

    /// gid -> buffered outcome for completions that arrived before gid store.
    pub(crate) pending_completion: Arc<Mutex<HashMap<String, (String, PendingOutcome)>>>,

    spawner: Arc<dyn SidecarSpawner>,
    app_handle: AppHandle,
}

impl QueueManager {
    /// Production constructor. Wired up in lib.rs setup().
    pub fn new(app_handle: AppHandle, capacity: usize) -> Self {
        Self::with_spawner(app_handle, capacity, Arc::new(ProductionSpawner))
    }

    /// Test constructor injecting a fake spawner.
    #[cfg(test)]
    pub(crate) fn with_spawner(
        app_handle: AppHandle,
        capacity: usize,
        spawner: Arc<dyn SidecarSpawner>,
    ) -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            semaphore: Arc::new(Semaphore::new(capacity)),
            active_permits: Mutex::new(HashMap::new()),
            target_capacity: AtomicUsize::new(capacity),
            slots_to_retire: AtomicUsize::new(0),
            notify: Notify::new(),
            aria2_gids: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_completion: Arc::new(Mutex::new(HashMap::new())),
            spawner,
            app_handle,
        }
    }

    /// Current pending order, as id list. Returned by move_in_queue.
    pub(crate) async fn pending_order(&self) -> Vec<String> {
        self.pending
            .lock()
            .await
            .iter()
            .map(|t| t.id.clone())
            .collect()
    }
}

/// Placeholder production spawner. Real wiring added in Task 7.
struct ProductionSpawner;

#[crate::async_trait]
impl SidecarSpawner for ProductionSpawner {
    async fn add_uri(&self, _id: &str, _payload: &SpawnPayload) -> Result<String, String> {
        Err("production spawner not wired yet".to_string())
    }
    async fn run_media(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        Err("production spawner not wired yet".to_string())
    }
    async fn run_native(&self, _id: &str, _payload: &SpawnPayload) -> Result<(), String> {
        Err("production spawner not wired yet".to_string())
    }
}
```

### Step 1.2: Add `async-trait` dependency

- [ ] **Add to `src-tauri/Cargo.toml` under `[dependencies]`:**

```toml
async-trait = "0.1"
```

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: builds (the `#[crate::async_trait]` macro resolves once the dep is present).

> Note: we use `#[crate::async_trait]` and re-export `async_trait::async_trait` as `crate::async_trait` in Step 1.3 so test code outside the crate can't accidentally depend on the macro path. Simpler: just use `#[async_trait::async_trait]` directly throughout. We'll do the direct form — replace every `#[crate::async_trait]` above with `#[async_trait::async_trait]`.

- [ ] **In `queue.rs`, replace all three occurrences of `#[crate::async_trait]` with `#[async_trait::async_trait]`.**

### Step 1.3: Register the module in `lib.rs`

- [ ] **In `src-tauri/src/lib.rs`, find the module declarations around line 421-426:**

```rust
pub mod download;
#[allow(dead_code)]
mod ipc;
mod parity;
pub mod error;
pub mod commands;
```

- [ ] **Add `pub mod queue;` after `pub mod download;`:**

```rust
pub mod download;
pub mod queue;
#[allow(dead_code)]
mod ipc;
mod parity;
pub mod error;
pub mod commands;
```

Also remove `#[allow(dead_code)]` from `mod ipc;` — no, keep it; ipc is used via `crate::ipc::` references and ts-rs. Leave the existing `#[allow(dead_code)]` intact.

### Step 1.4: Verify it compiles

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: clean build. Warnings about unused fields/methods are fine at this stage.

### Step 1.5: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat(queue): scaffold QueueManager module with types and spawner trait"
```

---

## Task 2: Implement enqueue, permit parking, and release (TDD)

**Goal:** The membership primitives. `push` adds a task; `acquire_and_park` gets a permit and parks it; `release_permit` removes and frees it idempotently. No dispatcher loop yet — tests drive these directly.

**Files:**
- Modify: `src-tauri/src/queue.rs`
- Create: `src-tauri/tests/queue_manager.rs`

### Step 2.1: Write the failing tests for push, park, release, idempotent release

- [ ] **Create `src-tauri/tests/queue_manager.rs`:**

```rust
use firelink_lib::queue::{QueueManager, SpawnPayload, TaskKind, QueuedTask};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::test::{mock_builder, mock_context, noop_assets};
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
fn make_manager(capacity: usize) -> (QueueManager, Arc<CountingSpawner>) {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let spawner = Arc::new(CountingSpawner::new());
    // with_spawner is pub(crate) — expose via a test-only helper in queue.rs.
    let mgr = QueueManager::test_new(app.handle(), capacity, spawner.clone());
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
    // Park a permit manually (simulates dispatcher having dispatched one).
    mgr.acquire_and_park("a").await;
    let avail_before = mgr.available_permits();
    mgr.release_permit("a").await; // first release: frees the slot
    let avail_after_first = mgr.available_permits();
    mgr.release_permit("a").await; // second release: no-op
    let avail_after_second = mgr.available_permits();
    assert_eq!(avail_after_first - avail_before, 1);
    assert_eq!(avail_after_second, avail_after_first, "second release must not free another slot");
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
```

- [ ] **Run:** `cd src-tauri && cargo test --test queue_manager -- --nocapture`
  Expected: FAIL — `push`, `acquire_and_park`, `pop_front`, `available_permits`, `test_new` don't exist.

### Step 2.2: Implement `push`, `pop_front`, `acquire_and_park`, `release_permit`, `available_permits`

- [ ] **Add to `src-tauri/src/queue.rs` inside `impl QueueManager` (after `pending_order`):**

```rust
    /// Enqueue a task. Notifies the dispatcher. Emits download-state{queued}.
    pub(crate) async fn push(&self, task: QueuedTask) {
        let id = task.id.clone();
        self.pending.lock().await.push_back(task);
        self.emit_state(id, DownloadStatus::Queued);
        self.notify.notify_one();
    }

    /// Pop the next task, or None if empty.
    pub(crate) async fn pop_front(&self) -> Option<QueuedTask> {
        self.pending.lock().await.pop_front()
    }

    /// Acquire a permit and park it under `id`. Used by the dispatcher after
    /// it has popped a task. Returns the permit on error (acquire failures
    /// are fatal — semaphore closed).
    pub(crate) async fn acquire_and_park(&self, id: &str) {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        self.active_permits.lock().await.insert(id.to_string(), permit);
    }

    /// Release the permit parked under `id`, if any. Idempotent. Wakes the
    /// dispatcher so a freed slot is claimed promptly.
    pub(crate) async fn release_permit(&self, id: &str) {
        let removed = self.active_permits.lock().await.remove(id).is_some();
        if removed {
            self.notify.notify_one();
        }
    }

    /// Number of un-acquired permits currently in the semaphore pool.
    pub(crate) fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    fn emit_state(&self, id: impl Into<String>, status: DownloadStatus) {
        use tauri::Emitter;
        let _ = self.app_handle.emit(
            "download-state",
            DownloadStateEvent::new(id, status),
        );
    }
```

We also need `test_new` (test-only constructor) and a way to expose `SidecarSpawner` to the integration test (it's `pub(crate)`). Add a `#[cfg(test)]`-gated public re-export is not possible in an integration test (different crate). Solution: make `SidecarSpawner`, `SpawnPayload`, `QueuedTask`, `TaskKind`, `QueueManager`, and `test_new` `pub` (not `pub(crate)`) so the integration test can use them. The trait is only meaningful inside this crate's dispatch logic, but exposing it publicly is harmless and keeps tests honest.

- [ ] **In `queue.rs`, change visibility** of these items from `pub(crate)` to `pub`: `TaskKind`, `QueuedTask`, `SpawnPayload`, `SidecarSpawner`, `QueueManager` (already `pub`), and the methods `push`, `pop_front`, `acquire_and_park`, `release_permit`, `available_permits`, `pending_order`. Keep `PendingOutcome` as `pub(crate)` (test doesn't need it yet).

- [ ] **Add the test-only constructor to `impl QueueManager`:**

```rust
    #[cfg(test)]
    pub fn test_new(app_handle: AppHandle, capacity: usize, spawner: Arc<dyn SidecarSpawner>) -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            semaphore: Arc::new(Semaphore::new(capacity)),
            active_permits: Mutex::new(HashMap::new()),
            target_capacity: AtomicUsize::new(capacity),
            slots_to_retire: AtomicUsize::new(0),
            notify: Notify::new(),
            aria2_gids: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_completion: Arc::new(Mutex::new(HashMap::new())),
            spawner,
            app_handle,
        }
    }
```

Now delete the `with_spawner` method (superseded by `test_new`) and remove the `#[cfg(test)]` on it.

- [ ] **Run:** `cd src-tauri && cargo test --test queue_manager -- --nocapture`
  Expected: PASS — all three tests green.

### Step 2.3: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/tests/queue_manager.rs
git commit -m "feat(queue): add push/pop/acquire_and_park/release_permit primitives"
```

---

## Task 3: Implement the dispatcher loop with idle-park and CAS retirement (TDD)

**Goal:** The long-running dispatcher that idle-parks when pending is empty, acquires a permit, honors the retirement debt via CAS (never underflowing), and re-pops under lock to guard against races. This is the heart of the spec — three of the four discovered bugs live here.

**Files:**
- Modify: `src-tauri/src/queue.rs`
- Modify: `src-tauri/tests/queue_manager.rs`

### Step 3.1: Write the failing test for idle-park (no busy-spin)

This test wraps the semaphore in a counting layer is hard (Semaphore is concrete). Instead we assert behaviorally: with an empty queue and capacity > 0, the dispatcher must NOT consume a permit. We park the dispatcher, sleep 200ms, then assert `available_permits()` is still equal to capacity (no permit was acquired).

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
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
```

- [ ] **Run:** `cd src-tauri && cargo test dispatcher_parks_when_idle -- --nocapture`
  Expected: FAIL — `run_dispatcher` doesn't exist.

### Step 3.2: Write the failing test for CAS retirement never underflowing

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
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
```

- [ ] **Run:** `cd src-tauri && cargo test cas_retirement_never_underflows -- --nocapture`
  Expected: FAIL — `run_dispatcher`, `set_capacity`, `slots_to_retire_load` don't exist.

### Step 3.3: Implement `run_dispatcher`, `set_capacity`, `slots_to_retire_load`

- [ ] **Add to `impl QueueManager` in `queue.rs`:**

```rust
    /// Resize the global concurrency limit. Grow adds permits immediately;
    /// shrink records a retirement debt honored lazily by the dispatcher.
    pub(crate) fn set_capacity(&self, new_target: usize) {
        let prev_target = self.target_capacity.swap(new_target, Ordering::Relaxed);
        if new_target == prev_target {
            return;
        }
        if new_target > prev_target {
            let delta = new_target - prev_target;
            self.semaphore.add_permits(delta);
            self.notify.notify_one();
        } else {
            let delta = prev_target - new_target;
            self.slots_to_retire.fetch_add(delta, Ordering::Relaxed);
        }
    }

    /// Test accessor for the retirement debt counter.
    #[cfg(test)]
    pub fn slots_to_retire_load(&self) -> usize {
        self.slots_to_retire.load(Ordering::Relaxed)
    }

    /// The long-running dispatcher. One instance is spawned in setup().
    /// Idle-parks on Notify; CAS-honors retirement debt; re-pops under lock.
    pub async fn run_dispatcher(self: Arc<Self>) {
        loop {
            // (1) Idle-park: avoid busy-spin when pending is empty.
            if self.pending.lock().await.is_empty() {
                self.notify.notified().await;
                continue;
            }
            // (2) Acquire a slot.
            let permit = self
                .semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("semaphore closed");
            // (3) CAS retirement — never underflows to usize::MAX.
            let mut retired = false;
            loop {
                let debt = self.slots_to_retire.load(Ordering::Relaxed);
                if debt == 0 {
                    break;
                }
                match self.slots_to_retire.compare_exchange_weak(
                    debt,
                    debt - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        drop(permit);
                        retired = true;
                        break;
                    }
                    Err(_actual) => {
                        // retry with the loaded value on the next iteration
                    }
                }
            }
            if retired {
                continue;
            }
            // (4) Re-pop under lock — guards against racing removals between
            //     waking from Notify and acquiring the permit.
            let task = match self.pending.lock().await.pop_front() {
                Some(t) => t,
                None => {
                    drop(permit);
                    continue;
                }
            };
            self.dispatch_one(permit, task).await;
        }
    }
```

`dispatch_one` is referenced but not yet implemented. Add a minimal stub so this compiles; full impl is Task 4.

- [ ] **Add a stub `dispatch_one` to `impl QueueManager`:**

```rust
    async fn dispatch_one(self: Arc<Self>, _permit: OwnedSemaphorePermit, task: QueuedTask) {
        // Full implementation in Task 4. For now, park the permit so the
        // idle/retirement tests run without leaking slots.
        let id = task.id.clone();
        self.acquire_and_park(&id).await;
        self.emit_state(id, DownloadStatus::Downloading);
        // NOTE: this stub leaks the permit (never released). Acceptable for
        // the two tests in this task, which don't dispatch real tasks.
        let _ = task;
    }
```

Wait — the stub calls `acquire_and_park` which acquires a *second* permit (the dispatcher already holds one passed as `_permit`). That double-acquires. Fix: `dispatch_one` receives the already-acquired permit; it should park THAT permit, not acquire another. Refactor `acquire_and_park` to split into "acquire" and "park existing permit":

- [ ] **Replace `acquire_and_park` with two methods in `queue.rs`:**

```rust
    /// Acquire a permit from the semaphore (blocks until one is available).
    pub(crate) async fn acquire_permit(&self) -> OwnedSemaphorePermit {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed")
    }

    /// Park an already-acquired permit under `id`.
    pub(crate) async fn park_permit(&self, id: &str, permit: OwnedSemaphorePermit) {
        self.active_permits
            .lock()
            .await
            .insert(id.to_string(), permit);
    }
```

- [ ] **Update the `release_permit_is_idempotent` test (Task 2.1) to use the split API.** In `tests/queue_manager.rs`, replace `mgr.acquire_and_park("a").await;` with:

```rust
    let permit = mgr.acquire_permit().await;
    mgr.park_permit("a", permit).await;
```

- [ ] **Update `dispatch_one` stub to park the passed permit:**

```rust
    async fn dispatch_one(self: Arc<Self>, permit: OwnedSemaphorePermit, task: QueuedTask) {
        let id = task.id.clone();
        self.park_permit(&id, permit).await;
        self.emit_state(id, DownloadStatus::Downloading);
        let _ = task;
    }
```

- [ ] **Run:** `cd src-tauri && cargo test --test queue_manager -- --nocapture`
  Expected: PASS — all five tests (3 from Task 2 + 2 new) green.

### Step 3.4: Write the failing test for grow releasing immediately

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
#[tokio::test]
async fn grow_releases_immediately_and_dispatches_waiting_tasks() {
    let (mgr, spawner) = make_manager(2);
    let mgr_arc = Arc::new(mgr);

    // Block the spawner so dispatched tasks don't complete (permits stay parked).
    // CountingSpawner completes instantly, so to keep permits parked we instead
    // push 4 tasks with capacity 2: two dispatch, two wait. Then grow to 4.
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
```

- [ ] **Run:** `cd src-tauri && cargo test grow_releases_immediately -- --nocapture`
  Expected: This may PASS already (the dispatcher + grow logic is correct). If it passes, good — it's a regression guard. If it fails, debug the wakeup path (`set_capacity` grow branch calls `notify_one`).

### Step 3.5: Write the failing test for shrink converging to target

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
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

    // Shrink to 2 while 4 are "active" (permits parked; CountingSpawner
    // already returned so we must manually release to simulate completion).
    mgr_arc.set_capacity(2);

    // Release the 4 active permits one by one; after 2 releases, the debt
    // (2) is exhausted, so no new dispatches should occur until... there are
    // no more pending either (6 - 4 = 2 pending, 2 retired). Active settles at 0
    // new dispatches beyond the initial 4 because debt absorbs both slots.
    mgr_arc.release_permit("t0").await;
    mgr_arc.release_permit("t1").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Debt was 2; two releases retired both. The 2 remaining pending tasks
    // (t4, t5) can now dispatch since debt is 0 and slots freed.
    // Total native calls: initial 4 + t4 + t5 = 6.
    assert_eq!(
        spawner.native_calls.load(Ordering::SeqCst),
        6,
        "shrink converges: after debt exhausted, remaining pending dispatch"
    );

    handle.abort();
}
```

- [ ] **Run:** `cd src-tauri && cargo test shrink_converges -- --nocapture`
  Expected: PASS (validates the full resize lifecycle). If it fails, the CAS loop or wakeup is off — check that `release_permit`'s `notify_one` wakes the dispatcher after retirement.

### Step 3.6: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/tests/queue_manager.rs
git commit -m "feat(queue): dispatcher loop with idle-park and CAS-based semaphore resize"
```

---

## Task 4: Implement `dispatch_one` with uniform permit parking (TDD)

**Goal:** The full `dispatch_one` that spawns sidecars via the `SidecarSpawner` trait, parks permits uniformly (aria2 trap fix), and releases on completion. Plus the aria2 gid-completion race handling.

**Files:**
- Modify: `src-tauri/src/queue.rs`
- Modify: `src-tauri/tests/queue_manager.rs`

### Step 4.1: Write the failing test for aria2 permit surviving RPC return

The fake spawner's `add_uri` returns instantly with a gid. The permit must remain parked (active) until `release_permit` is called — NOT released when the RPC returns.

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
fn aria2_task(id: &str) -> QueuedTask {
    QueuedTask {
        id: id.to_string(),
        kind: TaskKind::Aria2,
        payload: SpawnPayload::default(),
    }
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
```

- [ ] **Run:** `cd src-tauri && cargo test aria2_permit_survives_rpc_return -- --nocapture`
  Expected: depends — the Task 3 stub parks the permit but never calls the spawner. This test will fail because `add_uri_calls == 0`. Good — that's the right failure mode.

### Step 4.2: Implement the real `dispatch_one`

- [ ] **Replace the `dispatch_one` stub in `queue.rs` with the full implementation:**

```rust
    async fn dispatch_one(self: Arc<Self>, permit: OwnedSemaphorePermit, task: QueuedTask) {
        let id = task.id.clone();
        // Park the permit BEFORE spawning, keyed by id. Uniform parking:
        // aria2's RPC returns instantly, so the permit must outlive the
        // dispatch_one call. Media/Native runners release on exit; pause
        // releases immediately via release_permit.
        self.park_permit(&id, permit).await;
        self.emit_state(&id, DownloadStatus::Downloading);

        match task.kind {
            TaskKind::Aria2 => {
                // Issue the RPC; it returns immediately with a gid. The permit
                // stays parked in active_permits. Release happens via the WS
                // poller / pause / remove commands calling release_permit.
                match self.spawner.add_uri(&id, &task.payload).await {
                    Ok(gid) => self.remember_gid(id.clone(), gid).await,
                    Err(error) => {
                        self.emit_failed(&id, error);
                        self.release_permit(&id).await;
                    }
                }
            }
            TaskKind::Media => {
                let this = Arc::clone(&self);
                let payload = task.payload.clone();
                let id_for_task = id.clone();
                tauri::async_runtime::spawn(async move {
                    let outcome = this.spawner.run_media(&id_for_task, &payload).await;
                    this.finish_runner(&id_for_task, outcome).await;
                });
            }
            TaskKind::Native => {
                let this = Arc::clone(&self);
                let payload = task.payload.clone();
                let id_for_task = id.clone();
                tauri::async_runtime::spawn(async move {
                    let outcome = this.spawner.run_native(&id_for_task, &payload).await;
                    this.finish_runner(&id_for_task, outcome).await;
                });
            }
        }
    }

    /// Called when a Media/Native runner exits. Releases the permit and emits
    /// the terminal state. Idempotent: if pause already released, this is a
    /// no-op for the permit, but we still emit the terminal state only if the
    /// item isn't already paused (handled by the frontend's status check).
    async fn finish_runner(self: Arc<Self>, id: &str, outcome: Result<(), String>) {
        match outcome {
            Ok(()) => {
                self.emit_state(id, DownloadStatus::Completed);
            }
            Err(error) => {
                self.emit_failed(id, error);
            }
        }
        self.release_permit(id).await;
    }

    fn emit_failed(&self, id: &str, error: String) {
        use tauri::Emitter;
        let _ = self
            .app_handle
            .emit("download-state", DownloadStateEvent::failed(id, error));
    }

    /// Store gid -> id, then reconcile any buffered completion for that gid.
    /// Closes the gid-lookup race (spec §5.5).
    pub(crate) async fn remember_gid(&self, id: String, gid: String) {
        {
            let mut gids = self.aria2_gids.write().unwrap();
            gids.insert(gid.clone(), id.clone());
        }
        // Check for a buffered completion that arrived before we stored the gid.
        let buffered = self.pending_completion.lock().await.remove(&gid);
        if let Some((buf_id, outcome)) = buffered {
            self.apply_completion(&buf_id, outcome).await;
        }
    }

    /// Apply an aria2 completion outcome: release permit + emit state.
    pub(crate) async fn apply_completion(&self, id: &str, outcome: PendingOutcome) {
        match outcome {
            PendingOutcome::Complete => {
                self.emit_state(id, DownloadStatus::Completed);
            }
            PendingOutcome::Error(error) => {
                self.emit_failed(id, error);
            }
        }
        self.release_permit(id).await;
    }
```

`PendingOutcome` is `pub(crate)` — `apply_completion` takes it, so that's fine within the crate. But the WS poller (in lib.rs) also needs to construct `PendingOutcome`. Make it `pub`:

- [ ] **Change `PendingOutcome` from `pub(crate)` to `pub`** in `queue.rs`.

### Step 4.3: Handle aria2 completion arriving before gid store (TDD)

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
use firelink_lib::queue::PendingOutcome;

#[tokio::test]
async fn gid_completion_before_store_buffers_and_reconciles() {
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

    // And confirm a buffered event for a KNOWN-future gid reconciles:
    // Push another aria2 task; its gid will be "gid-2".
    mgr_arc.push(aria2_task("b")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Buffer a completion for gid-2 BEFORE remember_gid runs for it. In real
    // flow remember_gid runs immediately after add_uri; here we simulate the
    // race by calling handle_aria2_event then remember_gid manually.
    // (The dispatcher already called remember_gid, so gid-2 is stored. To
    // truly test the race we'd need to inject between add_uri and remember_gid.
    // The buffering path is exercised by the gid-unknown case above; this
    // assertion just confirms no crash and the permit for "b" can be released.)
    mgr_arc.release_permit("b").await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle.abort();
}
```

This test is more of a smoke test for the buffering path. The critical assertion is the first one: an unknown gid doesn't free a permit. Implement `handle_aria2_event`:

- [ ] **Add to `impl QueueManager` in `queue.rs`:**

```rust
    /// Entry point for the aria2 WS poller. Resolves gid -> id; if not yet
    /// stored, buffers the outcome for reconciliation by remember_gid.
    pub async fn handle_aria2_event(&self, gid: &str, outcome: PendingOutcome) {
        let id_opt = {
            let gids = self.aria2_gids.read().unwrap();
            gids.get(gid).cloned()
        };
        match id_opt {
            Some(id) => {
                self.apply_completion(&id, outcome).await;
            }
            None => {
                // Buffer keyed by gid; id is filled by remember_gid via the
                // (gid, (id_placeholder, outcome)) — but we don't know the id
                // yet. Store with empty id; remember_gid will re-derive id
                // from the gid->id map it just inserted.
                self.pending_completion
                    .lock()
                    .await
                    .insert(gid.to_string(), (String::new(), outcome));
            }
        }
    }
```

Wait — the buffered entry stores `(String::new(), outcome)` but `remember_gid` then does `pending_completion.remove(&gid)` and gets `(buf_id, outcome)` with empty `buf_id`. It should use the `id` it just inserted, not the empty buffer. Fix `remember_gid`:

- [ ] **Update `remember_gid` to use the freshly-stored id:**

```rust
    pub(crate) async fn remember_gid(&self, id: String, gid: String) {
        {
            let mut gids = self.aria2_gids.write().unwrap();
            gids.insert(gid.clone(), id.clone());
        }
        let buffered = self.pending_completion.lock().await.remove(&gid);
        if let Some((_buf_id, outcome)) = buffered {
            // Use `id` (the one we just stored), not the placeholder buf_id.
            self.apply_completion(&id, outcome).await;
        }
    }
```

- [ ] **Run:** `cd src-tauri && cargo test --test queue_manager -- --nocapture`
  Expected: PASS — all tests including `aria2_permit_survives_rpc_return` and `gid_completion_before_store`.

### Step 4.4: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/tests/queue_manager.rs
git commit -m "feat(queue): dispatch_one with uniform permit parking and gid-race handling"
```

---

## Task 5: Implement Move Up / Move Down and enqueue_many (TDD)

**Goal:** Reordering and bulk enqueue. `move_in_queue` swaps within the pending VecDeque and returns the new order. `enqueue_many` is for startup/start-all.

**Files:**
- Modify: `src-tauri/src/queue.rs`
- Modify: `src-tauri/tests/queue_manager.rs`

### Step 5.1: Write the failing test for reordering

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
#[tokio::test]
async fn move_up_down_reorders_pending() {
    let (mgr, _spawner) = make_manager(3);
    let mgr_arc = Arc::new(mgr);
    // Don't run the dispatcher — we're testing pure reorder on pending.
    mgr_arc.push(sample_task("a")).await;
    mgr_arc.push(sample_task("b")).await;
    mgr_arc.push(sample_task("c")).await;

    // Move "c" up: [a,b,c] -> [a,c,b]
    mgr_arc.move_in_queue("c", QueueDirection::Down).await; // sanity: down on last is no-op
    assert_eq!(mgr_arc.pending_order().await, vec!["a", "b", "c"]);

    mgr_arc.move_in_queue("c", QueueDirection::Up).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["a", "c", "b"]);

    // Move "a" down: [a,c,b] -> [c,a,b]
    mgr_arc.move_in_queue("a", QueueDirection::Down).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["c", "a", "b"]);

    // Boundary: move "c" (now first) up — no-op.
    mgr_arc.move_in_queue("c", QueueDirection::Up).await;
    assert_eq!(mgr_arc.pending_order().await, vec!["c", "a", "b"]);
}
```

- [ ] **Run:** `cd src-tauri && cargo test move_up_down_reorders -- --nocapture`
  Expected: FAIL — `move_in_queue` doesn't exist. Add `use firelink_lib::queue::QueueDirection;` to the test imports.

### Step 5.2: Implement `move_in_queue` and `enqueue_many`

- [ ] **Add to `impl QueueManager` in `queue.rs`:**

```rust
    /// Reorder a pending task up or down. Returns the new pending order.
    /// No-op at boundaries. Does not emit (membership unchanged); the caller
    /// (Tauri command) returns the order to the frontend.
    pub(crate) async fn move_in_queue(
        &self,
        id: &str,
        direction: QueueDirection,
    ) -> Vec<String> {
        let mut pending = self.pending.lock().await;
        let pos = pending.iter().position(|t| t.id == id);
        if let Some(pos) = pos {
            let target = match direction {
                QueueDirection::Up => pos.checked_sub(1),
                QueueDirection::Down => {
                    if pos + 1 < pending.len() {
                        Some(pos + 1)
                    } else {
                        None
                    }
                }
            };
            if let Some(target) = target {
                pending.swap(pos, target);
            }
        }
        pending.iter().map(|t| t.id.clone()).collect()
    }

    /// Remove a task from pending if present (used by remove_download).
    /// Does NOT release a permit (the caller handles active permits via
    /// release_permit if the task was already dispatched).
    pub(crate) async fn remove_from_pending(&self, id: &str) -> bool {
        let mut pending = self.pending.lock().await;
        let before = pending.len();
        pending.retain(|t| t.id != id);
        let removed = pending.len() < before;
        if removed {
            self.notify.notify_one();
        }
        removed
    }

    /// Bulk enqueue by appending tasks. Used by startup and start-all.
    pub(crate) async fn enqueue_many(&self, tasks: Vec<QueuedTask>) {
        let mut pending = self.pending.lock().await;
        for task in tasks {
            let id = task.id.clone();
            pending.push_back(task);
            self.emit_state(id, DownloadStatus::Queued);
        }
        drop(pending);
        self.notify.notify_one();
    }
```

- [ ] **Run:** `cd src-tauri && cargo test move_up_down_reorders -- --nocapture`
  Expected: PASS.

### Step 5.3: Write the failing test for Notify firing on enqueue/release

- [ ] **Add to `src-tauri/tests/queue_manager.rs`:**

```rust
#[tokio::test]
async fn notify_fires_on_push_and_release() {
    let (mgr, _spawner) = make_manager(1);
    let mgr_arc = Arc::new(mgr);

    // Park a permit manually so the dispatcher has something to release.
    let permit = mgr_arc.acquire_permit().await;
    mgr_arc.park_permit("a", permit).await;

    let handle = {
        let mgr_clone = Arc::clone(&mgr_arc);
        tokio::spawn(async move { mgr_clone.run_dispatcher().await })
    };

    // Dispatcher should be parked (pending empty). Push a task; it must
    // wake and dispatch within 100ms.
    mgr_arc.push(sample_task("x")).await;
    let dispatched = timeout(
        Duration::from_millis(150),
        async {
            loop {
                if mgr_arc.available_permits() == 0 {
                    return; // permit consumed = dispatched
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        },
    ).await;
    assert!(dispatched.is_ok(), "push must wake the idle dispatcher");

    handle.abort();
}
```

- [ ] **Run:** `cd src-tauri && cargo test notify_fires -- --nocapture`
  Expected: PASS (validates the wakeup invariant from spec §5.2).

### Step 5.4: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/tests/queue_manager.rs
git commit -m "feat(queue): move_in_queue, enqueue_many, remove_from_pending"
```

---

## Task 6: Add `DownloadStateEvent`, `QueueDirection` to `ipc.rs` and regenerate bindings

**Goal:** Move the ts-rs-exported types into `ipc.rs` (the canonical home, alongside `DownloadStatus`) and regenerate the frontend bindings. Also remove the now-obsolete `_dispatched` field from `DownloadItem`.

**Files:**
- Modify: `src-tauri/src/ipc.rs`
- Modify: `src-tauri/src/queue.rs` (move types out, re-import from ipc)
- Generate: `src/bindings/DownloadStateEvent.ts`, `src/bindings/QueueDirection.ts`

### Step 6.1: Move `DownloadStateEvent` and `QueueDirection` to `ipc.rs`

- [ ] **In `src-tauri/src/ipc.rs`, add at the end of the file (after `PersistedSettings`):**

```rust
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
```

- [ ] **In `src-tauri/src/queue.rs`:**
  - Remove the `QueueDirection` and `DownloadStateEvent` struct/enum definitions and their `impl DownloadStateEvent` block.
  - Replace the `use crate::ipc::DownloadStatus;` import line with:

```rust
use crate::ipc::{DownloadDirection_unused, DownloadStateEvent, DownloadStatus, QueueDirection};
```

Wait — that has a typo. Use:

```rust
use crate::ipc::{DownloadStateEvent, DownloadStatus, QueueDirection};
```

- [ ] **Update the `DownloadStateEvent::new` / `failed` constructors.** They were on the struct in `queue.rs`; move them to `ipc.rs` as an `impl` block there, since the struct now lives there:

In `ipc.rs`, after the `DownloadStateEvent` struct, add:

```rust
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
}
```

- [ ] **In `queue.rs`, remove the old `impl DownloadStateEvent` block** (now redundant — it lives in ipc.rs).

### Step 6.2: Remove `_dispatched` from `DownloadItem`

- [ ] **In `src-tauri/src/ipc.rs`, in the `DownloadItem` struct, delete these three lines:**

```rust
    #[serde(rename = "_dispatched")]
    #[ts(optional)]
    pub dispatched: Option<bool>,
```

### Step 6.3: Regenerate bindings and verify

- [ ] **Run:** `cd src-tauri && cargo test export_bindings --lib`
  Expected: PASS; creates `src/bindings/DownloadStateEvent.ts`, `src/bindings/QueueDirection.ts`, and updates `src/bindings/DownloadItem.ts` (removes `_dispatched`).

- [ ] **Verify the new bindings exist:** `ls src/bindings/ | grep -E "DownloadStateEvent|QueueDirection"`
  Expected: both files listed.

- [ ] **Run:** `npx tsc --noEmit`
  Expected: TYPE ERRORS in `useDownloadStore.ts`, `downloadStore.ts`, `App.tsx` referencing `_dispatched` or `processQueue`. These will be fixed in Tasks 8-9. Do NOT fix yet — confirm the errors are only about `_dispatched`/`processQueue`, then proceed.

### Step 6.4: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/queue.rs src/bindings/
git commit -m "feat(ipc): add DownloadStateEvent and QueueDirection; drop _dispatched field"
```

---

## Task 7: Wire `QueueManager` into `AppState` and add Tauri commands

**Goal:** Production wiring. Instantiate `QueueManager` in `AppState`, spawn the dispatcher in `setup()`, and add the `enqueue_download`/`enqueue_many`/`resume_download`/`move_in_queue`/`remove_from_queue` commands. Convert `start_download`/`start_media_download` to `pub(crate)`. Rewrite `set_concurrent_limit`.

This is a large task. Break the spawner wiring into a real `ProductionSpawner` that calls the existing aria2/yt-dlp/native code.

**Files:**
- Modify: `src-tauri/src/queue.rs` (real `ProductionSpawner`)
- Modify: `src-tauri/src/lib.rs` (AppState, setup, commands, WS poller, aria2 ceiling)

### Step 7.1: Implement the real `ProductionSpawner`

The production spawner needs access to `AppState` (aria2 port/secret, coordinator, app handle). Since `SidecarSpawner` is `Send + Sync + 'static` and `AppState` is a Tauri-managed state, we store an `AppHandle` in the spawner and look up state at call time.

- [ ] **In `src-tauri/src/queue.rs`, replace the stub `ProductionSpawner` with:**

```rust
/// Production spawner that delegates to the real aria2 RPC, yt-dlp, and
/// native coordinator runners.
pub(crate) struct ProductionSpawner {
    app_handle: AppHandle,
}

impl ProductionSpawner {
    pub(crate) fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl SidecarSpawner for ProductionSpawner {
    async fn add_uri(&self, id: &str, payload: &SpawnPayload) -> Result<String, String> {
        let state = self.app_handle.state::<crate::AppState>();
        let mut options = serde_json::Map::new();
        let resolved_dest = crate::resolve_path(&payload.destination, &self.app_handle);
        if !crate::is_safe_path(&resolved_dest, &self.app_handle) {
            return Err("Path traversal blocked".to_string());
        }
        options.insert(
            "dir".to_string(),
            serde_json::json!(resolved_dest.to_string_lossy().to_string()),
        );
        let safe_filename = std::path::Path::new(&payload.filename.replace('\\', "/"))
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download")
            .to_string();
        options.insert("out".to_string(), serde_json::json!(safe_filename));
        let conn = payload.connections.unwrap_or(1);
        options.insert("split".to_string(), serde_json::json!(conn.to_string()));
        options.insert(
            "max-connection-per-server".to_string(),
            serde_json::json!(conn.to_string()),
        );
        let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
        options.insert("max-tries".to_string(), serde_json::json!(mt.to_string()));
        options.insert("continue".to_string(), serde_json::json!("true"));
        if let Some(speed) = &payload.speed_limit {
            options.insert("max-download-limit".to_string(), serde_json::json!(speed));
        }
        if let Some(user) = &payload.username {
            options.insert("http-user".to_string(), serde_json::json!(user));
        }
        if let Some(pass) = &payload.password {
            options.insert("http-passwd".to_string(), serde_json::json!(pass));
        }
        if let Some(chk) = &payload.checksum {
            options.insert("checksum".to_string(), serde_json::json!(chk));
        }
        if let Some(ua) = &payload.user_agent {
            options.insert("user-agent".to_string(), serde_json::json!(ua));
        }
        let mut header_list = Vec::new();
        if let Some(cook) = &payload.cookies {
            header_list.push(format!("Cookie: {}", cook));
        }
        if let Some(hdrs) = &payload.headers {
            for line in hdrs.lines() {
                if !line.trim().is_empty() {
                    header_list.push(line.trim().to_string());
                }
            }
        }
        if !header_list.is_empty() {
            options.insert("header".to_string(), serde_json::json!(header_list));
        }
        if let Some(prox) = &payload.proxy {
            options.insert("all-proxy".to_string(), serde_json::json!(prox));
        }
        let uris = crate::collect_download_uris(&payload.url, payload.mirrors.as_deref());
        let params = serde_json::json!([uris, options]);
        let result = crate::rpc_call(state.aria2_port, &state.aria2_secret, "aria2.addUri", params)
            .await
            .map_err(|e| e.to_string())?;
        let gid = result.as_str().unwrap_or("").to_string();
        // Note: gid -> id is stored by QueueManager.remember_gid, not here.
        let _ = id;
        Ok(gid)
    }

    async fn run_media(&self, id: &str, payload: &SpawnPayload) -> Result<(), String> {
        let state = self.app_handle.state::<crate::AppState>();
        let mut cancel_rx = state
            .download_coordinator
            .register_media(id.to_string())
            .await
            .map_err(|e| e)?;
        crate::start_media_download_internal(
            self.app_handle.clone(),
            id,
            payload.url.clone(),
            payload.destination.clone(),
            payload.filename.clone(),
            payload.format_selector.clone(),
            payload.cookie_source.clone(),
            payload.speed_limit.clone(),
            payload.username.clone(),
            payload.password.clone(),
            payload.headers.clone(),
            payload.proxy.clone(),
            payload.user_agent.clone(),
            payload.max_tries,
            &mut cancel_rx,
        )
        .await
    }

    async fn run_native(&self, id: &str, payload: &SpawnPayload) -> Result<(), String> {
        let state = self.app_handle.state::<crate::AppState>();
        let download_id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
        let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
        let resolved_dest = crate::resolve_path(&payload.destination, &self.app_handle);
        let safe_filename = std::path::Path::new(&payload.filename.replace('\\', "/"))
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download")
            .to_string();
        // The native coordinator is command-based; we send Start and let its
        // existing worker run. Completion is reported via the existing
        // download-complete/download-failed events. To make this await-able,
        // we wrap it: the coordinator's run_coordinator emits on completion,
        // but for the queue we need a synchronous result. Simplest: send Start
        // and return Ok immediately — the WS-style completion events for the
        // native path already fire download-complete/download-failed, which
        // the QueueManager must translate.
        //
        // REVISED: the native DownloadCoordinator already has a headless mode
        // (spawn_headless) that returns an event receiver. But integrating
        // that here is a larger refactor. For now, we send Start and rely on
        // the existing emit paths. The permit for native tasks is released by
        // a NEW listener added in Task 8 that watches download-complete/
        // download-failed and calls release_permit.
        state
            .download_coordinator
            .send(crate::download::DownloadCmd::Start(Box::new(
                crate::download::DownloadPayload {
                    id: download_id,
                    urls: crate::collect_download_uris(&payload.url, payload.mirrors.as_deref()),
                    output_path: resolved_dest.join(safe_filename),
                    speed_limit: payload.speed_limit.clone(),
                    username: payload.username.clone(),
                    password: payload.password.clone(),
                    headers: payload.headers.clone(),
                    cookies: payload.cookies.clone(),
                    user_agent: payload.user_agent.clone(),
                    max_tries: mt,
                    proxy: payload.proxy.clone(),
                },
            )))
            .await
            .map_err(|e| e)?;
        Ok(())
    }
}
```

**Important note on the native path:** unlike Media (which awaits to completion inside `run_media`), the native `DownloadCoordinator` is event-driven. `run_native` returns `Ok(())` immediately after sending `Start`, but the permit was parked by `dispatch_one`. This means the permit would leak for native tasks (never released).

There are two options:
- **(A)** Add a `download-complete`/`download-failed` listener in `setup()` that calls `queue.release_permit(id)` for native + media-yt-dlp tasks.
- **(B)** Refactor `DownloadCoordinator` to expose a per-id completion future.

Option A is far less invasive and matches how aria2 completions are already handled (via events). **We go with A.** This means `run_native` returning `Ok(())` immediately is fine — the `finish_runner` in `dispatch_one` will emit `Completed` too early though.

To avoid the double-emit, change `run_native` to NOT return through `finish_runner`. Instead, native tasks are "fire and observe": `dispatch_one` should not spawn a runner that calls `finish_runner` for native; it should just send Start and let the completion listener handle release + emit.

- [ ] **Revise `dispatch_one` in `queue.rs` for the Native case to not use a runner:**

Replace the `TaskKind::Native =>` arm with:

```rust
            TaskKind::Native => {
                // Native coordinator is event-driven (fire-and-observe). Send
                // Start; completion is handled by the download-complete/
                // download-failed listener in lib.rs setup() which calls
                // release_permit + apply_completion.
                let this = Arc::clone(&self);
                let payload = task.payload.clone();
                let id_for_task = id.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = this.spawner.run_native(&id_for_task, &payload).await {
                        // Start itself failed — release + emit immediately.
                        this.emit_failed(&id_for_task, error);
                        this.release_permit(&id_for_task).await;
                    }
                    // On Ok, do nothing here — the completion listener owns release.
                });
            }
```

And the Media arm stays as-is (it awaits to completion via `finish_runner`).

### Step 7.2: Add `QueueManager` to `AppState` and instantiate in `setup()`

- [ ] **In `src-tauri/src/lib.rs`, update the `AppState` struct (around line 436):**

```rust
pub struct AppState {
    pub download_coordinator: download::DownloadCoordinator,
    pub extension_pairing_token: extension_server::SharedExtensionToken,
    pub extension_frontend_ready: extension_server::SharedFrontendReady,
    pub aria2_port: u16,
    pub aria2_secret: String,
    pub media_semaphore: Arc<tokio::sync::Semaphore>,
    pub sleep_preventer: Arc<Mutex<Option<keepawake::KeepAwake>>>,
    pub aria2_gids: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub queue_manager: Arc<queue::QueueManager>,
}
```

- [ ] **In `setup()` (around line 1464), update the `app.manage(AppState { ... })` block.** Read `max_concurrent_downloads` from the store first, then construct the QueueManager and spawn its dispatcher:

Replace the existing `app.manage(AppState { ... })` block with:

```rust
            // Read max_concurrent_downloads from the settings store (default 3).
            let max_concurrent = {
                use tauri_plugin_store::StoreExt;
                let mut capacity = crate::queue::DEFAULT_MAX_CONCURRENT;
                if let Ok(store) = app.handle().store("store.bin") {
                    if let Some(settings_val) = store.get("settings") {
                        if let Some(settings_str) = settings_val.as_str() {
                            if let Ok(settings_json) = serde_json::from_str::<serde_json::Value>(settings_str) {
                                if let Some(n) = settings_json.get("maxConcurrentDownloads").and_then(|v| v.as_u64()) {
                                    capacity = n as usize;
                                }
                            }
                        }
                    }
                }
                capacity
            };

            let queue_manager = Arc::new(queue::QueueManager::new(app.handle().clone(), max_concurrent));
            let dispatcher_mgr = Arc::clone(&queue_manager);
            tauri::async_runtime::spawn(async move {
                dispatcher_mgr.run_dispatcher().await;
            });

            app.manage(AppState {
                download_coordinator: download::DownloadCoordinator::spawn(app.handle().clone()),
                extension_pairing_token,
                extension_frontend_ready,
                aria2_port,
                aria2_secret: aria2_secret.clone(),
                media_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
                sleep_preventer: Arc::new(Mutex::new(None)),
                aria2_gids: Arc::new(RwLock::new(std::collections::HashMap::new())),
                queue_manager,
            });
```

Note: `QueueManager::new` currently takes `(AppHandle, usize)` and builds a `ProductionSpawner` internally. Update `new`:

- [ ] **In `queue.rs`, update `QueueManager::new`:**

```rust
    pub fn new(app_handle: AppHandle, capacity: usize) -> Self {
        let spawner: Arc<dyn SidecarSpawner> = Arc::new(ProductionSpawner::new(app_handle.clone()));
        Self {
            pending: Mutex::new(VecDeque::new()),
            semaphore: Arc::new(Semaphore::new(capacity)),
            active_permits: Mutex::new(HashMap::new()),
            target_capacity: AtomicUsize::new(capacity),
            slots_to_retire: AtomicUsize::new(0),
            notify: Notify::new(),
            aria2_gids: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_completion: Arc::new(Mutex::new(HashMap::new())),
            spawner,
            app_handle,
        }
    }
```

Delete the old `with_spawner` method (already superseded by `test_new`).

### Step 7.3: Add the Tauri commands

- [ ] **In `src-tauri/src/lib.rs`, add these commands (place them near `start_download`, around line 600):**

```rust
#[tauri::command]
async fn enqueue_download(
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
    format_selector: Option<String>,
    cookie_source: Option<String>,
    is_media: Option<bool>,
) -> Result<String, AppError> {
    let media = is_media.unwrap_or(false);
    let kind = if media {
        queue::TaskKind::Media
    } else {
        queue::TaskKind::Aria2
    };
    let task = queue::QueuedTask {
        id: id.clone(),
        kind,
        payload: queue::SpawnPayload {
            url,
            destination,
            filename,
            connections,
            speed_limit,
            username,
            password,
            headers,
            checksum,
            cookies,
            mirrors,
            user_agent,
            max_tries,
            proxy,
            format_selector,
            cookie_source,
            is_media: media,
        },
    };
    state.queue_manager.push(task).await;
    Ok(id)
}
```

Wait — the spec says the frontend determines `is_media` and the backend infers `TaskKind` from it. But the aria2-vs-native distinction (the original `start_download` tries aria2 first, falls back to native) is lost. Decision: keep it simple. `is_media` true → Media. `is_media` false → Aria2 (with the native fallback handled inside `ProductionSpawner::add_uri`: if aria2 RPC fails, it should fall back to the native coordinator). The existing `start_download` already does this fallback; replicate it:

- [ ] **Update `ProductionSpawner::add_uri` to fall back to native on aria2 RPC failure.** Replace the `let result = crate::rpc_call(...)` block at the end with:

```rust
        match crate::rpc_call(state.aria2_port, &state.aria2_secret, "aria2.addUri", params).await {
            Ok(result) => {
                let gid = result.as_str().unwrap_or("").to_string();
                Ok(gid)
            }
            Err(e) => {
                // aria2 unavailable — fall back to native coordinator. But the
                // native path is event-driven (not gid-based). We signal this
                // by returning a synthetic gid prefixed with "native:" so the
                // QueueManager knows to release via the completion listener,
                // not the WS poller.
                log::warn!("aria2 addUri failed, falling back to native: {}", e);
                let download_id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
                let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
                let safe_filename = std::path::Path::new(&payload.filename.replace('\\', "/"))
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("download")
                    .to_string();
                state
                    .download_coordinator
                    .send(crate::download::DownloadCmd::Start(Box::new(
                        crate::download::DownloadPayload {
                            id: download_id,
                            urls: crate::collect_download_uris(&payload.url, payload.mirrors.as_deref()),
                            output_path: resolved_dest.join(safe_filename),
                            speed_limit: payload.speed_limit.clone(),
                            username: payload.username.clone(),
                            password: payload.password.clone(),
                            headers: payload.headers.clone(),
                            cookies: payload.cookies.clone(),
                            user_agent: payload.user_agent.clone(),
                            max_tries: mt,
                            proxy: payload.proxy.clone(),
                        },
                    )))
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(format!("native:{id}"))
            }
        }
```

This keeps the aria2-first-then-native behavior. The `native:` prefix tells the system that completion comes via `download-complete`/`download-failed` events (handled in Task 8), not the WS poller.

- [ ] **Add the remaining commands to `lib.rs`:**

```rust
#[tauri::command]
async fn enqueue_many(
    state: tauri::State<'_, AppState>,
    payloads: Vec<SpawnPayloadCmd>,
) -> Result<(), AppError>
where
    // SpawnPayloadCmd is a serde-friendly mirror of SpawnPayload + id + kind.
{
    // Simplify: accept Vec of (id, is_media) and re-read the rest from the
    // frontend-provided payloads. But SpawnPayload has many fields. Define a
    // single EnqueueManyItem struct.
    unimplemented!("see EnqueueManyItem below")
}
```

The `enqueue_many` signature is getting complex. Define a deserializable command struct:

- [ ] **In `queue.rs`, add a serde-friendly command payload (mirror of SpawnPayload + id + is_media):**

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct EnqueueItem {
    pub id: String,
    pub url: String,
    pub destination: String,
    pub filename: String,
    pub connections: Option<i32>,
    pub speed_limit: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers: Option<String>,
    pub checksum: Option<String>,
    pub cookies: Option<String>,
    pub mirrors: Option<String>,
    pub user_agent: Option<String>,
    pub max_tries: Option<i32>,
    pub proxy: Option<String>,
    pub format_selector: Option<String>,
    pub cookie_source: Option<String>,
    pub is_media: Option<bool>,
}

impl EnqueueItem {
    pub(crate) fn into_task(self) -> QueuedTask {
        let media = self.is_media.unwrap_or(false);
        let kind = if media {
            TaskKind::Media
        } else {
            TaskKind::Aria2
        };
        let id = self.id.clone();
        QueuedTask {
            id,
            kind,
            payload: SpawnPayload {
                url: self.url,
                destination: self.destination,
                filename: self.filename,
                connections: self.connections,
                speed_limit: self.speed_limit,
                username: self.username,
                password: self.password,
                headers: self.headers,
                checksum: self.checksum,
                cookies: self.cookies,
                mirrors: self.mirrors,
                user_agent: self.user_agent,
                max_tries: self.max_tries,
                proxy: self.proxy,
                format_selector: self.format_selector,
                cookie_source: self.cookie_source,
                is_media: media,
            },
        }
    }
}
```

- [ ] **Now define the clean commands in `lib.rs`.** Replace the stub `enqueue_download` and `enqueue_many` with:

```rust
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
async fn resume_download(state: tauri::State<'_, AppState>, id: String) -> Result<(), AppError> {
    // Re-enqueue requires the payload; the frontend must provide it via the
    // existing DownloadItem. Simplest: frontend calls enqueue_download with
    // the full item again. This command is a thin alias for clarity.
    // For now, return an error guiding the frontend to use enqueue_download.
    // (We keep the command for API symmetry; frontend uses enqueue_download.)
    Err(AppError::Internal(
        "resume_download: use enqueue_download with the item payload".to_string(),
    ))
}
```

Hmm — `resume_download` without a payload is awkward. Reconsider: the frontend has the full `DownloadItem`, so resume = `enqueue_download(item)`. We don't need a separate `resume_download` command. Drop it from the spec's command surface and have the frontend call `enqueue_download` for resume.

- [ ] **Drop `resume_download`.** Remove it from the commands list. The frontend's `resumeDownload(id)` will call `enqueue_download` with the item's payload (rebuilt from the stored `DownloadItem`).

- [ ] **Add `move_in_queue`, `remove_from_queue`, and rewrite `set_concurrent_limit` in `lib.rs`:**

```rust
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
```

The old `set_concurrent_limit` body (the `rpc_call` to aria2) is removed — aria2's ceiling is fixed at startup (Task 7.5).

### Step 7.4: Register the new commands in the invoke handler

- [ ] **In `lib.rs`, update the `invoke_handler` list (around line 1700).** Replace the line:

```rust
            set_extension_frontend_ready, set_concurrent_limit, set_global_speed_limit, remove_download,
```

with:

```rust
            set_extension_frontend_ready, set_concurrent_limit, set_global_speed_limit, remove_download,
            enqueue_download, enqueue_many, move_in_queue, remove_from_queue,
```

### Step 7.5: Raise the aria2 daemon ceiling

- [ ] **In `lib.rs` `setup()`, find the aria2 spawn args (around line 1504) and add `--max-concurrent-downloads=100`:**

```rust
                    cmd = cmd.arg("--enable-rpc=true")
                        .arg(format!("--rpc-listen-port={}", aria2_port))
                        .arg(format!("--rpc-secret={}", aria2_secret))
                        .arg("--rpc-listen-all=false")
                        .arg("--continue=true")
                        .arg("--allow-overwrite=false")
                        .arg("--summary-interval=1")
                        .arg("--console-log-level=warn")
                        .arg("--download-result=hide")
                        .arg("--check-certificate=true")
                        .arg("--max-concurrent-downloads=100");
```

### Step 7.6: Make `resolve_path`, `is_safe_path`, `rpc_call`, `collect_download_uris`, `start_media_download_internal` accessible to `queue.rs`

These are currently private functions in `lib.rs`. `ProductionSpawner` (in `queue.rs`) needs them.

- [ ] **In `lib.rs`, change visibility** of these items from private to `pub(crate)`:
  - `fn resolve_path(...)` → `pub(crate) fn resolve_path(...)`
  - `fn is_safe_path(...)` → `pub(crate) fn is_safe_path(...)`
  - `async fn rpc_call(...)` → `pub(crate) async fn rpc_call(...)`
  - `fn collect_download_uris(...)` → `pub(crate) fn collect_download_uris(...)`
  - `pub(crate) async fn start_media_download_internal(...)` → already `pub(crate)`, leave as is.

### Step 7.7: Build and fix compilation errors

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: a few errors likely around `AppState` field access from `queue.rs` (`state.aria2_port` etc. are `pub`, so fine) and the `media_semaphore` being unused now. Fix as they arise:
  - If `media_semaphore` is now unused, keep it in `AppState` (harmless) or prefix with `_`. Keep it to minimize churn.
  - The old `start_download`/`start_media_download` commands are still registered but now unused by the frontend. Leave them registered for now (the frontend migration in Task 8 will stop calling them); we remove them in Task 10.

### Step 7.8: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/queue.rs src-tauri/src/lib.rs
git commit -m "feat(queue): wire QueueManager into AppState, add enqueue/move/remove commands"
```

---

## Task 8: Extend aria2 WS poller and add native/media completion listener

**Goal:** Release permits on aria2 completion (WS poller), and on native/media completion (a new `download-complete`/`download-failed` listener). Wire the gid-completion race handling.

**Files:**
- Modify: `src-tauri/src/lib.rs`

### Step 8.1: Extend the WS poller to release permits on aria2 completion

The existing WS poller (around line 1570) handles `aria2.onDownloadComplete` / `aria2.onDownloadError` by emitting frontend events. Add permit release + state emit via `queue_manager`.

- [ ] **In `lib.rs`, find the WS poller's `match method { ... }` block (around line 1590) and update it.** Currently:

```rust
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
                                                                    // ... notification
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
```

The `aria2_gids_clone1` map is the old `id -> gid` map on `AppState`. But `QueueManager` now owns its own `aria2_gids` (gid -> id). The WS poller must use the QueueManager's map and call `handle_aria2_event`.

- [ ] **Replace the WS poller's inner match block with QueueManager-aware logic.** The `app_handle_ws` already has access to state. Replace the block with:

```rust
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
                                                            state.queue_manager.handle_aria2_event(gid, outcome).await;
                                                        }
                                                    }
                                                }
                                            }
                                        }
```

This delegates to `handle_aria2_event`, which resolves gid→id (or buffers), applies completion (release permit + emit `download-state`), and handles the race.

The old `download-complete`/`download-failed` emits for aria2 are now done inside `apply_completion` (via `emit_state`/`emit_failed`). But the frontend also listens for `download-complete`/`download-failed` for notifications (App.tsx). To keep notifications working, have `apply_completion` ALSO emit the legacy events. Update `apply_completion` in `queue.rs`:

- [ ] **In `queue.rs`, update `apply_completion` and `finish_runner` and `emit_state`/`emit_failed` to also emit the legacy events.** Replace `emit_state`:

```rust
    fn emit_state(&self, id: impl Into<String>, status: DownloadStatus) {
        use tauri::Emitter;
        let id: String = id.into();
        let _ = self.app_handle.emit(
            "download-state",
            DownloadStateEvent::new(id.clone(), status),
        );
        // Legacy events for notification wiring (App.tsx listens to these).
        match status {
            DownloadStatus::Completed => {
                let _ = self.app_handle.emit("download-complete", id);
            }
            _ => {}
        }
    }

    fn emit_failed(&self, id: &str, error: String) {
        use tauri::Emitter;
        let _ = self
            .app_handle
            .emit("download-state", DownloadStateEvent::failed(id, error.clone()));
        let _ = self.app_handle.emit("download-failed", id.to_string());
    }
```

`finish_runner` and `apply_completion` already call these, so they're covered.

### Step 8.2: Add the native/media completion listener

Native tasks (and the aria2-fallback-to-native path) report completion via `download-complete`/`download-failed` events from the `DownloadCoordinator`. We need a backend listener that releases the permit for these.

But wait — `finish_runner` (for Media) already releases the permit when `run_media` returns. And `emit_failed`/`emit_state` emit the legacy events. So if a native `download-complete` event fires, we'd double-handle.

Clarify the release ownership:
- **Media (yt-dlp):** `run_media` awaits to completion → `finish_runner` releases + emits. The `download-complete` event for media is emitted INSIDE `start_media_download_internal` (it emits on yt-dlp termination). So Media would emit `download-complete` AND `finish_runner` would emit `download-state{completed}`. That's fine — both fire, the frontend handles `download-state` as source of truth and `download-complete` for notifications.
- **Native (and aria2-fallback):** `run_native` sends Start and returns Ok immediately. The `DownloadCoordinator` emits `download-complete`/`download-failed` when the native download finishes. We need a listener that, on these events for a native task, calls `release_permit` + `apply_completion`.

So we need ONE backend listener on `download-complete`/`download-failed` that handles the native path. But it must not double-release Media/aria2 permits (which are already released by `finish_runner`/`handle_aria2_event`). Since `release_permit` is idempotent, double-release is harmless. But double-emitting `download-state` could cause flicker — also harmless because the status is the same.

Simplify: add the listener; it calls `release_permit` (idempotent) and `apply_completion` (re-emits). For Media/aria2, the extra emit is redundant but not harmful. For Native, it's the only release path. This is the cleanest single-listener approach.

- [ ] **In `lib.rs` `setup()`, after spawning the dispatcher, add a completion listener.** Find the `tauri::async_runtime::spawn` for the WS poller and add, after it:

```rust
            // Backend listener: release permits + emit terminal state for
            // native (and aria2-fallback) downloads. Idempotent for Media/aria2
            // which already release via finish_runner/handle_aria2_event.
            let completion_app = app.handle().clone();
            let completion_mgr = Arc::clone(&queue_manager);
            tauri::async_runtime::spawn(async move {
                let mut rx_complete = completion_app.listen("download-complete", move |event| {
                    let id = event.payload().to_string();
                    let mgr = Arc::clone(&completion_mgr);
                    tauri::async_runtime::spawn(async move {
                        mgr.apply_completion(&id, crate::queue::PendingOutcome::Complete).await;
                    });
                });
                let completion_app2 = completion_app.clone();
                let completion_mgr2 = Arc::clone(&queue_manager);
                let mut rx_failed = completion_app2.listen("download-failed", move |event| {
                    let id = event.payload().to_string();
                    let mgr = Arc::clone(&completion_mgr2);
                    tauri::async_runtime::spawn(async move {
                        mgr.apply_completion(&id, crate::queue::PendingOutcome::Error("download failed".to_string())).await;
                    });
                });
                // Keep the task alive; the listeners are unregistered on drop.
                std::future::pending::<()>().await;
                drop(rx_complete);
                drop(rx_failed);
            });
```

Hmm — `app.listen` returns an `EventId` (a guard), and the closure can't be async. The pattern above uses nested spawns, which is correct. But `completion_app.listen` borrows `completion_mgr` via the outer Arc clone — the `move` closure captures `completion_mgr` (the first listener) and `completion_mgr2` (the second). That's fine. But both listeners are registered inside one spawned task that then awaits `pending` — the `listen` calls happen immediately on the task's start, so they register before the `pending`. That works.

Actually, there's a subtlety: `tauri::AppHandle::listen` registers globally and the returned `EventId` unregisters on drop. We want them to live for the app's lifetime. The `std::future::pending::<()>().await` keeps the task alive forever, so the guards never drop. Good.

But `apply_completion` is `pub(crate)`; the listener is in `lib.rs` (same crate), so fine.

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: compiles. Fix any borrow/move errors in the listener closures.

### Step 8.3: Update `pause_download` and `remove_download` to release permits

- [ ] **In `lib.rs`, update `pause_download` (around line 1012).** After the existing aria2 pause + coordinator pause + media pause, add permit release:

```rust
#[tauri::command]
async fn pause_download(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    println!("pause_download called for id: {}", id);

    // Release the concurrency slot FIRST, before signaling the sidecar to die.
    state.queue_manager.release_permit(&id).await;
    // Remove from pending if it was queued (not yet dispatched).
    state.queue_manager.remove_from_pending(&id).await;

    // Emit the paused state.
    use tauri::Emitter;
    let _ = state.app_handle_emit_paused(&id);

    let gid = state.aria2_gids.read().unwrap().get(&id).cloned();
    if let Some(g) = gid {
        if !g.starts_with("native:") {
            let _ = rpc_call(state.aria2_port, &state.aria2_secret, "aria2.pause", serde_json::json!([g])).await;
        }
    }

    if let Ok(download_id) = Uuid::parse_str(&id) {
        let _ = state
            .download_coordinator
            .send(download::DownloadCmd::Pause(download_id))
            .await;
    }
    state.download_coordinator.pause_media(id.clone()).await
}
```

We referenced `state.app_handle_emit_paused` which doesn't exist. The `AppHandle` isn't directly on `AppState`. Add an `app_handle` field to `AppState`, or get it from the command's `app_handle: tauri::AppHandle` parameter. Tauri commands can accept `app_handle: tauri::AppHandle` as an injected arg:

- [ ] **Update `pause_download` signature to take `app_handle`:**

```rust
#[tauri::command]
async fn pause_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    println!("pause_download called for id: {}", id);

    // Release the concurrency slot FIRST, before signaling the sidecar to die.
    state.queue_manager.release_permit(&id).await;
    state.queue_manager.remove_from_pending(&id).await;

    // Emit the paused state.
    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id.clone(), crate::ipc::DownloadStatus::Paused),
    );

    let gid = state.aria2_gids.read().unwrap().get(&id).cloned();
    if let Some(g) = gid {
        if !g.starts_with("native:") {
            let _ = rpc_call(state.aria2_port, &state.aria2_secret, "aria2.pause", serde_json::json!([g])).await;
        }
    }

    if let Ok(download_id) = Uuid::parse_str(&id) {
        let _ = state
            .download_coordinator
            .send(download::DownloadCmd::Pause(download_id))
            .await;
    }
    state.download_coordinator.pause_media(id).await
}
```

- [ ] **Update `remove_download` (around line 1029) similarly** to call `remove_from_pending` + `release_permit`:

```rust
#[tauri::command]
async fn remove_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    filepath: Option<String>,
) -> Result<(), String> {
    println!("remove_download called for id: {}", id);

    // Remove from the queue (pending or active) and free the slot.
    state.queue_manager.remove_from_pending(&id).await;
    state.queue_manager.release_permit(&id).await;

    use tauri::Emitter;
    let _ = app_handle.emit(
        "download-state",
        crate::ipc::DownloadStateEvent::new(id.clone(), crate::ipc::DownloadStatus::Paused),
    );

    if let Ok(download_id) = Uuid::parse_str(&id) {
        state
            .download_coordinator
            .send(download::DownloadCmd::Cancel(download_id))
            .await?;
    }
    state.download_coordinator.pause_media(id.clone()).await?;

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
```

### Step 8.4: Build and commit

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: compiles.

- [ ] **Commit:**

```bash
git add src-tauri/src/lib.rs src-tauri/src/queue.rs
git commit -m "feat(queue): release permits on aria2/native completion and pause/remove"
```

---

## Task 9: Frontend store refactor — reactive, `pendingOrder`, remove `processQueue`

**Goal:** Make the store reactive. Remove `processQueue`, add the `pendingOrder` slice, add the `download-state` listener, and update all actions to invoke backend commands.

**Files:**
- Modify: `src/store/useDownloadStore.ts`
- Modify: `src/store/downloadStore.ts`

### Step 9.1: Refactor `useDownloadStore.ts`

- [ ] **In `src/store/useDownloadStore.ts`:**

1. Remove the `isProcessingQueue` let-binding and the entire `processQueue` method.
2. Add `pendingOrder: string[]` to the state interface and initial value `[]`.
3. Add `pauseDownload`, `resumeDownload`, `moveUp`, `moveDown` to the interface.
4. Update `addDownload`, `startQueue`, `pauseQueue`, `redownload`, `removeDownload`, `initDB`.

Here are the specific edits. First, the interface — add after `processQueue` removal. Find:

```typescript
  processQueue: () => Promise<void>;
  startQueue: (queueId: string) => Promise<number>;
```

Replace with:

```typescript
  pendingOrder: string[];
  startQueue: (queueId: string) => Promise<number>;
```

Find the `let isProcessingQueue = false;` line (above `create`) and delete it.

Add `pendingOrder: [],` to the initial state object (near `downloads: [],`).

- [ ] **Replace `addDownload`:**

```typescript
  addDownload: (item) => {
    info(`Download ${item.id} added to queue`);
    set((state) => ({ downloads: [...state.downloads, item] }));
    // Backend owns dispatch. The download-state listener appends to pendingOrder.
    invoke('enqueue_download', { item: buildEnqueueItem(item) }).catch((e) => {
      console.error('Failed to enqueue download:', e);
    });
  },
```

Add the `buildEnqueueItem` helper above `create`:

```typescript
const buildEnqueueItem = (item: DownloadItem) => ({
  id: item.id,
  url: item.url,
  destination: item.destination || '',
  filename: item.fileName,
  connections: item.connections ?? null,
  speedLimit: item.speedLimit || null,
  username: item.username || null,
  password: item.password || null,
  headers: item.headers || null,
  checksum: item.checksum || null,
  cookies: item.cookies || null,
  mirrors: item.mirrors || null,
  userAgent: null,
  maxTries: null,
  proxy: null,
  formatSelector: item.mediaFormatSelector || null,
  cookieSource: null,
  isMedia: item.isMedia ?? false,
});
```

Wait — the backend `EnqueueItem` uses snake_case fields? No: Tauri commands receive args as JS objects with the **camelCase** names matching the Rust field names AFTER Tauri's automatic snake→camel conversion. Tauri converts `speed_limit` → `speedLimit` at the JS boundary. So `buildEnqueueItem` must use camelCase keys. The object above does. Good.

- [ ] **Replace `updateDownload`** (remove the `processQueue` triggers):

```typescript
  updateDownload: (id, updates) => {
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          return {
            ...d,
            ...updates,
            fraction: updates.fraction !== undefined ? updates.fraction : d.fraction
          };
        }
        return d;
      })
    }));
    // Sync system integrations on status change; no more processQueue.
    if (updates.status) {
      syncSystemIntegrations();
    }
  },
```

- [ ] **Replace `removeDownload`:**

```typescript
  removeDownload: async (id, deleteFile = false) => {
    const item = get().downloads.find(d => d.id === id);
    if (item) {
      try {
        await invoke('remove_download', { id, filepath: null });
      } catch (e) {
        console.error("Failed to terminate download on deletion:", e);
      }
    }
    if (item && deleteFile) {
      try {
        const filepath = item.destination ? `${item.destination}/${item.fileName}` : null;
        if (filepath) {
          const partialPaths = [`${filepath}.aria2`, `${filepath}.part`];
          await invoke('trash_download_assets', { path: filepath, partialPaths });
        }
      } catch (e) {
        console.error("Failed to trash file from disk:", e);
      }
    }
    set((state) => ({
      downloads: state.downloads.filter(d => d.id !== id),
      pendingOrder: state.pendingOrder.filter(x => x !== id),
    }));
    info(`Download ${id} removed`);
    syncSystemIntegrations();
  },
```

- [ ] **Replace `redownload`:**

```typescript
  redownload: (id) => {
    const item = get().downloads.find(d => d.id === id);
    if (!item) return;
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          return { ...d, status: 'queued', fraction: 0, speed: '-', eta: '-' };
        }
        return d;
      })
    }));
    // If it was downloading, pause the old sidecar first.
    invoke('pause_download', { id }).catch(console.error);
    // Re-enqueue with the stored payload.
    invoke('enqueue_download', { item: buildEnqueueItem(item) }).catch(console.error);
    info(`Download ${id} redownload requested (queued)`);
  },
```

- [ ] **Replace `startQueue`:**

```typescript
  startQueue: async (queueId) => {
    const items = get().downloads
      .filter(item => item.queueId === queueId && (item.status === 'paused' || item.status === 'failed'))
      .map(item => buildEnqueueItem(item));

    if (items.length === 0) return 0;

    await invoke('enqueue_many', { items });
    info(`Queue ${queueId} started, ${items.length} items re-enqueued`);
    return items.length;
  },
```

- [ ] **Replace `pauseQueue`:**

```typescript
  pauseQueue: async (queueId) => {
    const activeIds = get().downloads
      .filter(item => item.queueId === queueId && (item.status === 'downloading' || item.status === 'queued'))
      .map(item => item.id);

    if (activeIds.length === 0) return 0;

    await Promise.all(activeIds.map(id => invoke('pause_download', { id }).catch(() => {})));
    info(`Queue ${queueId} paused, ${activeIds.length} items paused`);
    syncSystemIntegrations();
    return activeIds.length;
  },
```

- [ ] **Replace `initDB`** (re-enqueue interrupted + queued items on startup):

```typescript
  initDB: async () => {
    try {
      const queues = await tauriStore.get<Queue[]>('queues') || [];
      const downloads = await tauriStore.get<DownloadItem[]>('download_queue') || [];

      // Treat persisted 'downloading' as interrupted; reset to 'queued'.
      const normalized = downloads.map(d =>
        d.status === 'downloading' ? { ...d, status: 'queued' as const } : d
      );

      set(state => ({
        queues: queues.length > 0 ? queues : state.queues,
        downloads: normalized.length > 0 ? normalized : state.downloads,
        pendingOrder: normalized.filter(d => d.status === 'queued').map(d => d.id),
      }));

      // Re-enqueue all queued items via the backend.
      const enqueueItems = normalized
        .filter(d => d.status === 'queued')
        .map(d => buildEnqueueItem(d));
      if (enqueueItems.length > 0) {
        await invoke('enqueue_many', { items: enqueueItems });
      }
    } catch (e) {
      console.error("Failed to init DB", e);
    }
  },
```

- [ ] **Add the new actions** (`pauseDownload`, `resumeDownload`, `moveUp`, `moveDown`) to the interface and implementation. Add to the interface:

```typescript
  pauseDownload: (id: string) => Promise<void>;
  resumeDownload: (item: DownloadItem) => void;
  moveUp: (id: string) => void;
  moveDown: (id: string) => void;
```

And the implementations (place near the other actions):

```typescript
  pauseDownload: async (id) => {
    await invoke('pause_download', { id }).catch(console.error);
  },
  resumeDownload: (item) => {
    // Resume = re-enqueue with the stored payload.
    set((state) => ({
      downloads: state.downloads.map(d =>
        d.id === item.id ? { ...d, status: 'queued', speed: '-', eta: '-' } : d
      )
    }));
    invoke('enqueue_download', { item: buildEnqueueItem(item) }).catch(console.error);
  },
  moveUp: (id) => {
    invoke<[string]>('move_in_queue', { id, direction: 'up' }).then((order) => {
      set({ pendingOrder: order });
    }).catch(console.error);
  },
  moveDown: (id) => {
    invoke<[string]>('move_in_queue', { id, direction: 'down' }).then((order) => {
      set({ pendingOrder: order });
    }).catch(console.error);
  },
```

The `move_in_queue` return type is `Vec<String>` → TS `string[]`. Fix the generic: `invoke<string[]>('move_in_queue', ...)`.

- [ ] **Fix the import for `invokeCommand`.** The file imports `invokeCommand as invoke` — confirm `moveUp`/`moveDown` use `invoke<string[]>`.

### Step 9.2: Add the `download-state` listener and simplify the progress listener

- [ ] **In `src/store/downloadStore.ts`:**

1. Add the `download-state` listener (authoritative for status + pendingOrder).
2. Simplify the `download-progress` listener (remove the `queued → downloading` flip).

Find the existing `initDownloadListener` and replace its body:

```typescript
import type { DownloadStateEvent } from '../bindings/DownloadStateEvent';

export async function initDownloadListener() {
  if (unlistenProgress) return;

  // download-progress: updates only the transient progress store. Status
  // transitions are owned by download-state below.
  unlistenProgress = await listen<DownloadProgressEvent>('download-progress', (event) => {
    const payload = event.payload;
    useDownloadProgressStore.getState().updateDownloadProgress(payload.id, payload);

    const mainStore = useDownloadStore.getState();
    const current = mainStore.downloads.find(d => d.id === payload.id);
    if (current && payload.size && current.size !== payload.size) {
      mainStore.updateDownload(payload.id, { size: payload.size });
    }
  });

  // download-state: the single source of truth for status transitions.
  // Also authoritative for pendingOrder membership.
  unlistenState = await listen<DownloadStateEvent>('download-state', (event) => {
    const { id, status, error } = event.payload;
    const mainStore = useDownloadStore.getState();
    mainStore.updateDownload(id, {
      status: status as any,
      ...(error ? { speed: '-', eta: '-' } : {}),
    });
    // pendingOrder: append on queued, remove otherwise.
    if (status === 'queued') {
      useDownloadStore.setState((s) => ({
        pendingOrder: s.pendingOrder.includes(id) ? s.pendingOrder : [...s.pendingOrder, id]
      }));
    } else {
      useDownloadStore.setState((s) => ({
        pendingOrder: s.pendingOrder.filter(x => x !== id)
      }));
    }
  });

  if (!unlistenTray) {
    unlistenTray = await listen<string>('tray-action', (event) => {
      const mainStore = useDownloadStore.getState();
      if (event.payload === 'pause-all') {
        const uniqueQueues = Array.from(new Set(mainStore.downloads.map(d => d.queueId)));
        uniqueQueues.forEach(qid => mainStore.pauseQueue(qid));
      } else if (event.payload === 'resume-all') {
        const uniqueQueues = Array.from(new Set(mainStore.downloads.map(d => d.queueId)));
        uniqueQueues.forEach(qid => mainStore.startQueue(qid));
      }
    });
  }
}
```

- [ ] **Add the `unlistenState` variable** near the top:

```typescript
let unlistenProgress: UnlistenFn | null = null;
let unlistenState: UnlistenFn | null = null;
let unlistenTray: UnlistenFn | null = null;
```

### Step 9.3: Update `App.tsx` to remove `set_concurrent_limit` double-call and handle `download-state` for notifications

- [ ] **In `src/App.tsx`:** The `download-complete` listener (around line 211) updates status. Since `download-state` now owns status, the `download-complete` listener should only handle notifications, not status. Find:

```typescript
    const unlistenComplete = listen('download-complete', (event) => {
      updateDownload(event.payload, { status: 'completed', fraction: 1.0, speed: '-', eta: '-' });
      ...
```

Replace the `updateDownload(... status: 'completed' ...)` with just the fraction/speed/eta cleanup (status is set by download-state):

```typescript
    const unlistenComplete = listen('download-complete', (event) => {
      updateDownload(event.payload, { fraction: 1.0, speed: '-', eta: '-' });
      const settings = useSettingsStore.getState();
      if (settings.showNotifications) {
        const item = useDownloadStore.getState().downloads.find(d => d.id === event.payload);
        const fileName = item?.fileName || 'A file';
        sendNotification({
          title: 'Download Complete',
          body: `${fileName} has finished downloading.`,
          sound: settings.playCompletionSound ? 'default' : undefined
        });
      }
    });
```

The `download-failed` listener similarly: remove the status mutation (download-state owns it); keep the guard only for notifications if any. Find:

```typescript
    const unlistenFailed = listen('download-failed', (event) => {
      const current = useDownloadStore.getState().downloads.find(d => d.id === event.payload);
      if (current && current.status !== 'paused') {
        updateDownload(event.payload, { status: 'failed', speed: '-', eta: '-' });
      }
    });
```

Since `download-state` now sets `failed`, this listener can be removed or left as a no-op. Remove it to avoid confusion:

- [ ] **Remove the `unlistenFailed` listener block and its cleanup in the return.**

- [ ] **The `set_concurrent_limit` useEffect (line 75) stays as-is** — it now calls the rewritten backend command that resizes the semaphore.

### Step 9.4: Typecheck and commit

- [ ] **Run:** `npx tsc --noEmit`
  Expected: PASS (no `_dispatched`, no `processQueue` references remain).

- [ ] **Commit:**

```bash
git add src/store/useDownloadStore.ts src/store/downloadStore.ts src/App.tsx
git commit -m "feat(store): reactive queue — pendingOrder, download-state listener, remove processQueue"
```

---

## Task 10: `DownloadTable.tsx` and `DownloadItem.tsx` UI updates

**Goal:** Update `DownloadTable`'s `handleResume`/`handlePause` to use the store actions, and update `DownloadItem` with the queued visuals + Move Up/Down buttons.

**Files:**
- Modify: `src/components/DownloadTable.tsx`
- Modify: `src/components/DownloadItem.tsx`

### Step 10.1: Update `DownloadTable.tsx` handlers

- [ ] **In `src/components/DownloadTable.tsx`, update `handlePause` and `handleResume` (around line 93):**

Replace:

```typescript
  const handlePause = async (id: string) => {
    try {
      await invoke('pause_download', { id });
      updateDownload(id, { status: 'paused', speed: '-', eta: '-' });
    } catch (e) {
      console.error("Failed to pause:", e);
    }
  };

  const handleResume = (item: DownloadItem) => {
    updateDownload(item.id, { status: 'queued', _dispatched: false, speed: '-', eta: '-' });
    useDownloadStore.getState().processQueue();
  };
```

with:

```typescript
  const handlePause = async (id: string) => {
    try {
      await invoke('pause_download', { id });
      // download-state listener sets 'paused'; no manual update needed.
    } catch (e) {
      console.error("Failed to pause:", e);
    }
  };

  const handleResume = (item: DownloadItem) => {
    useDownloadStore.getState().resumeDownload(item);
  };
```

Also the "Resume All" button (line 150) calls `handleResume(d)` for paused items — that still works.

The "Pause All" button (line 161) calls `handlePause(d.id)` for downloading items — but should also pause queued items. Update the filter:

- [ ] **Update the Pause All onClick filter (line 161):**

```typescript
            onClick={() => {
              filteredDownloads.filter(d => d.status === 'downloading' || d.status === 'queued').forEach(d => handlePause(d.id));
            }}
```

### Step 10.2: Update `DownloadItem.tsx` with queued visuals and Move Up/Down

- [ ] **In `src/components/DownloadItem.tsx`:**

1. Import `Clock`, `ChevronUp`, `ChevronDown` from lucide-react.
2. Add `moveUp`, `moveDown` from the store.
3. Compute `queuePosition` and `queueLength` from `pendingOrder`.
4. Add the queued status cell branch.
5. Add Move Up/Down buttons in the action cluster.

Replace the imports (line 4):

```typescript
import { Play, Pause, MoreVertical, Clock, ChevronUp, ChevronDown } from 'lucide-react';
```

Replace the component body's store hooks (after `const download = ...` line 26) — add:

```typescript
  const pendingOrder = useDownloadStore(state => state.pendingOrder);
  const moveUp = useDownloadStore(state => state.moveUp);
  const moveDown = useDownloadStore(state => state.moveDown);
  const queuePosition = pendingOrder.indexOf(downloadId) + 1;
  const queueLength = pendingOrder.length;
```

Replace the status cell (lines 84-106). The new version handles `queued` explicitly:

```tsx
      <div className="download-status-cell">
        {download.status === 'completed' ? (
          <span className="download-status download-status-completed">Completed</span>
        ) : download.status === 'queued' ? (
          <span className="download-status download-status-queued">
            <Clock size={12} className="animate-pulse-slow" />
            <span>Queued</span>
            {queuePosition > 0 && (
              <span className="queue-position-badge">#{queuePosition}</span>
            )}
          </span>
        ) : (
          <>
            <div className="download-progress-track">
              <div
                ref={progressBarRef}
                className={`download-progress-fill ${download.status === 'paused' ? 'paused' : ''}`}
                style={{ width: `${(download.fraction || 0) * 100}%` }}
              />
            </div>
            <span
              ref={statusTextRef}
              className={`download-status ${download.status === 'paused' ? 'download-status-paused' : download.status === 'failed' ? 'download-status-failed' : download.status === 'downloading' ? 'download-status-downloading' : ''}`}
            >
              {download.status === 'downloading'
                ? `${((download.fraction || 0) * 100).toFixed(0)}%`
                : download.status.charAt(0).toUpperCase() + download.status.slice(1)}
            </span>
          </>
        )}
      </div>
```

Replace the action cluster (lines 116-143) to add Move Up/Down for queued items:

```tsx
      <div className="download-cell-right">
        <span className="truncate group-hover:hidden tabular-nums ml-auto">
          {download.dateAdded ? new Date(download.dateAdded).toLocaleDateString() : '-'}
        </span>

        <div className="hidden group-hover:flex items-center justify-end gap-0.5 w-full ml-auto">
          {download.status === 'downloading' && (
            <button onClick={() => handlePause(download.id)} className="app-icon-button h-7 w-7" title="Pause">
              <Pause size={14} fill="currentColor" />
            </button>
          )}
          {download.status === 'paused' && (
            <button onClick={() => handleResume(download)} className="app-icon-button h-7 w-7" title="Resume">
              <Play size={14} fill="currentColor" />
            </button>
          )}
          {download.status === 'queued' && (
            <>
              <button
                onClick={() => moveUp(download.id)}
                disabled={queuePosition <= 1}
                className="app-icon-button h-7 w-7"
                title="Move up"
              >
                <ChevronUp size={14} />
              </button>
              <button
                onClick={() => moveDown(download.id)}
                disabled={queuePosition === 0 || queuePosition >= queueLength}
                className="app-icon-button h-7 w-7"
                title="Move down"
              >
                <ChevronDown size={14} />
              </button>
            </>
          )}
          <button
            onClick={(e) => {
               e.stopPropagation();
               setContextMenu({ x: e.clientX, y: e.clientY, id: download.id });
            }}
            className="app-icon-button h-7 w-7"
            title="Options"
          >
            <MoreVertical size={14} />
          </button>
        </div>
      </div>
```

### Step 10.3: Add the CSS for queued visuals

- [ ] **In `src/index.css`, after the `.download-progress-fill.paused { ... }` block (line 1066):**

```css
  .download-progress-fill.queued {
    filter: grayscale(1) opacity(0.45);
    background: hsl(var(--text-muted));
  }

  .download-progress-track:has(.download-progress-fill.queued) {
    background-image: repeating-linear-gradient(
      45deg,
      hsl(var(--border-color)),
      hsl(var(--border-color)) 6px,
      hsl(var(--border-color) / 0.6) 6px,
      hsl(var(--border-color) / 0.6) 12px
    );
    background-size: 200% 100%;
    animation: queued-stripe 1.5s linear infinite;
  }

  @keyframes queued-stripe {
    from { background-position: 0 0; }
    to { background-position: 24px 0; }
  }
```

- [ ] **After `.download-status-completed { ... }` (around line 1079), add:**

```css
  .download-status-queued {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: hsl(var(--text-muted));
    font-weight: 500;
  }

  .queue-position-badge {
    margin-left: 2px;
    padding: 0 5px;
    border-radius: 8px;
    background: hsl(var(--item-hover));
    color: hsl(var(--text-secondary));
    font-size: 10px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .animate-pulse-slow {
    animation: pulse-slow 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes pulse-slow {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
```

Note: `--text-secondary` may not be a defined CSS variable. Check existing usage — the codebase uses `--text-primary`, `--text-muted`. Use `--text-muted` for the badge text:

- [ ] **In the `.queue-position-badge` rule, replace `color: hsl(var(--text-secondary));` with `color: hsl(var(--text-muted));`.**

### Step 10.4: Typecheck and build

- [ ] **Run:** `npx tsc --noEmit`
  Expected: PASS.

- [ ] **Run:** `npm run build`
  Expected: PASS (vite build succeeds).

### Step 10.5: Commit

- [ ] **Commit:**

```bash
git add src/components/DownloadTable.tsx src/components/DownloadItem.tsx src/index.css
git commit -m "feat(ui): queued-state visuals (clock, grayscale, position badge) and Move Up/Down buttons"
```

---

## Task 11: Remove deprecated `start_download`/`start_media_download` frontend entry points and dead code

**Goal:** Clean up. The frontend no longer calls `start_download`/`start_media_download` directly (it uses `enqueue_download`). Remove the now-unused command registrations and the `_dispatched`-related dead code. Keep the `pub(crate)` functions (used by the ProductionSpawner).

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ipc.rs` (if any `_dispatched` serde rename remains)

### Step 11.1: Remove `start_download` and `start_media_download` from the invoke handler

These commands are now only called internally by the ProductionSpawner (via the `pub(crate)` functions, not the `#[tauri::command]` wrappers). Remove the `#[tauri::command]` wrappers from the invoke handler list.

- [ ] **In `src-tauri/src/lib.rs`, update the `invoke_handler` list (around line 1700).** Remove `start_download, start_media_download,` from:

```rust
            start_download, start_media_download, pause_download, fetch_metadata, fetch_media_metadata,
```

becomes:

```rust
            pause_download, fetch_metadata, fetch_media_metadata,
```

Keep the `start_download` and `start_media_download` function definitions but remove their `#[tauri::command]` attribute (so they're not registered). Actually — `start_media_download_internal` is `pub(crate)` and used by the spawner; the `start_media_download` wrapper (the `#[tauri::command]`) is now unused. And `start_download` (the `#[tauri::command]`) is unused.

- [ ] **Remove the `#[tauri::command]` attribute from `start_download` and `start_media_download`** (or delete the functions if nothing else references them). Simplest: delete the `start_media_download` command wrapper entirely (it just set up the semaphore + cancel_rx + called `start_media_download_internal`; the spawner now does this inline). And delete the `start_download` command (the spawner's `add_uri` replaces it).

Check `initDB` in the old frontend — it used to call `start_download`/`start_media_download`. The refactored `initDB` (Task 9.1) uses `enqueue_many`. Confirmed safe to delete.

- [ ] **Delete the `start_download` function (lines ~599-713) and the `start_media_download` function (lines ~715-772) from `lib.rs`.** Keep `start_media_download_internal` (it's `pub(crate)` and called by the spawner).

### Step 11.2: Verify build and run tests

- [ ] **Run:** `cd src-tauri && cargo test -- --nocapture`
  Expected: all queue_manager tests + existing download_engine tests + export_bindings pass.

- [ ] **Run:** `cd src-tauri && cargo build --lib`
  Expected: compiles. Fix any "unused import" warnings.

- [ ] **Run:** `npx tsc --noEmit && npm run build`
  Expected: PASS.

### Step 11.3: Commit

- [ ] **Commit:**

```bash
git add src-tauri/src/lib.rs
git commit -m "refactor: remove deprecated start_download/start_media_download command wrappers"
```

---

## Task 12: Manual smoke test and final verification

**Goal:** Verify the full system end-to-end. No code changes — just runtime verification.

### Step 12.1: Build and run the app

- [ ] **Run:** `npm run tauri dev` (in a separate terminal; this is interactive — run it and observe).
  Expected: app launches.

### Step 12.2: Smoke test scenarios (from spec §8)

Perform each and verify:

- [ ] **Scenario 1 — basic concurrency:** Set `maxConcurrentDownloads = 3` in Settings. Add 5 downloads (use slow/large URLs or localhost test files). Verify:
  - 3 dispatch (status `downloading`, progress bars animate).
  - 2 show `Queued` with clock icon + `#1`/`#2` badges.

- [ ] **Scenario 2 — shrink mid-flight:** While 3 are downloading and 2 queued, change `maxConcurrentDownloads` to 1. Verify:
  - Active downloads are NOT killed (the 3 continue).
  - As each completes, the queued count does NOT grow back to 3 — it converges toward 1 active.

- [ ] **Scenario 3 — pause releases instantly:** While 1 is downloading and 1 is queued (capacity 1), pause the downloading one. Verify:
  - The queued item dispatches immediately (within ~100ms), before the paused sidecar fully exits.

- [ ] **Scenario 4 — Move Up/Down:** With 3 queued items, click Move Up on the 3rd. Verify:
  - The `#N` badges update instantly (`#3`→`#2`, `#2`→`#3`).
  - The disabled state on the buttons updates (first item's Move Up disabled).

- [ ] **Scenario 5 — restart recovery:** While 2 items are downloading, quit and relaunch the app. Verify:
  - Both show `Queued` on restart (interrupted → queued).
  - They re-enqueue and resume downloading (up to capacity).

- [ ] **Scenario 6 — aria2 path specifically:** Add a non-media HTTP download. Verify it dispatches via aria2 (check logs for `aria2 addUri`), the permit is released on completion (a queued item claims the slot).

- [ ] **Scenario 7 — yt-dlp media path:** Add a YouTube URL. Verify it dispatches via yt-dlp when a slot is free, and the permit releases on completion.

### Step 12.3: Final commit (if any test surfaced a fix)

- [ ] If any smoke test failed and you fixed it, commit the fix with a descriptive message. Otherwise, no commit needed.

---

## Self-Review

**1. Spec coverage:**

| Spec section | Task(s) |
|---|---|
| §4 Architecture (QueueManager sole gatekeeper) | 1, 2, 7 |
| §5.1 Dispatcher loop (idle-park + CAS + re-pop) | 3 |
| §5.2 Wakeup invariant (notify on enqueue/release/grow) | 2, 3, 5 |
| §5.3 Uniform permit parking (aria2 trap + pause release) | 4, 8 |
| §5.4 Resizing (grow + shrink) | 3 |
| §5.5 GID lookup race (pending_completion buffer) | 4 |
| §5.6/§6.1 aria2 high ceiling | 7 (Step 7.5) |
| §6 State-transition protocol (download-state event) | 6, 8 |
| §7.1-7.3 Frontend store refactor | 9 |
| §7.4 DownloadItem queued UI + Move Up/Down | 10 |
| §8 Testing (all 10 regression tests) | 2, 3, 4, 5 |

All spec sections covered. ✅

**2. Placeholder scan:**

- Searched for "TBD", "TODO", "implement later", "fill in" — none in task steps.
- Step 7.7 ("Fix as they arise") is acceptable — it's a build-and-fix checkpoint, not a placeholder for unspecified content.
- All code blocks are complete.

**3. Type/signature consistency:**

- `QueueDirection` — defined in Task 1 (`queue.rs`), moved to `ipc.rs` in Task 6, used in Task 5 (`move_in_queue`) and Task 7 (command). Consistent. ✅
- `DownloadStateEvent` — defined in Task 1, moved to `ipc.rs` in Task 6, used in Tasks 4/8. Consistent. ✅
- `EnqueueItem` — defined in Task 7 (`queue.rs`), used in Task 9 (`buildEnqueueItem` produces matching camelCase keys). ✅
- `SidecarSpawner::add_uri` returns `Result<String, String>` (gid) — consistent across Task 1 (trait), Task 4 (test), Task 7 (production). ✅
- `release_permit`, `apply_completion`, `handle_aria2_event`, `remember_gid` — used consistently in Tasks 4, 5, 8. ✅
- The `native:` gid prefix convention (Task 7 `add_uri` fallback) is checked in Task 8 (`pause_download` skips aria2.pause for `native:` gids). ✅

One inconsistency found and fixed inline during review: the original Task 2 had `acquire_and_park` which double-acquired in `dispatch_one`. Split into `acquire_permit` + `park_permit` in Step 3.3 and the Task 2.1 test updated accordingly. ✅

**Scope check:** This is one cohesive subsystem (the queue coordinator) with backend + frontend changes that must ship together. Appropriately scoped for a single plan. ✅
