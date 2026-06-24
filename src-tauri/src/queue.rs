use crate::ipc::{DownloadStateEvent, DownloadStatus, QueueDirection};
use crate::retry::{backoff_and_emit, is_transient_network_error, BackoffOutcome, MAX_RETRIES};
use log;
use serde::Deserialize;
use serde_json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore};
use ts_rs::TS;

/// Default capacity when no setting is read yet.
pub const DEFAULT_MAX_CONCURRENT: usize = 3;
pub const MEDIA_RUN_CANCELLED: &str = "__firelink_media_run_cancelled__";

/// Outcome of an aria2 completion that arrived before its gid was stored.
/// Carries the outcome so the correct state emit survives the race.
#[derive(Debug, Clone)]
pub enum PendingOutcome {
    Complete,
    Error(String),
}

/// What kind of sidecar a queued task spawns. Drives which runner the
/// dispatcher invokes.
#[derive(Debug, Clone)]
pub enum TaskKind {
    Aria2,
    Media,
    Native,
}

/// Everything needed to start a sidecar, captured at enqueue time so the
/// dispatcher can spawn it later without round-tripping back to the frontend.
#[derive(Debug, Clone)]
pub struct QueuedTask {
    pub id: String,
    pub queue_id: String,
    pub kind: TaskKind,
    pub payload: SpawnPayload,
}

/// Args mirroring start_download / start_media_download. Kept untyped-loose
/// (String/Option) to match the existing command signatures exactly.
#[derive(Debug, Clone, Default)]
pub struct SpawnPayload {
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

/// A sidecar spawner. In production this calls the real aria2/yt-dlp/native
/// runners; in tests it is replaced with a fake that records calls and
/// optionally hangs to simulate a long-running download.
#[async_trait::async_trait]
pub trait SidecarSpawner: Send + Sync + 'static {
    /// Spawn an aria2 download. Returns the gid. Must return quickly (the
    /// permit is already parked before this is called).
    async fn add_uri(&self, id: &str, payload: &SpawnPayload) -> Result<String, String>;

    /// Force-remove an aria2 gid created by a retry that raced with user
    /// cancellation.
    async fn remove_uri(&self, gid: &str) -> Result<(), String>;

    /// Run a media download to completion. The permit is parked for the full
    /// duration; release is handled by QueueManager on the runner's exit.
    async fn run_media(&self, id: &str, payload: &SpawnPayload) -> Result<(), String>;

    /// Run a native HTTP download to completion.
    async fn run_native(&self, id: &str, payload: &SpawnPayload) -> Result<(), String>;
}

/// The centralized concurrency gatekeeper. One instance lives in AppState.
pub struct QueueManager<R: tauri::Runtime = tauri::Wry> {
    registered_ids: Mutex<HashSet<String>>,
    pending: Mutex<VecDeque<QueuedTask>>,
    semaphore: Arc<Semaphore>,
    active_permits: Mutex<HashMap<String, OwnedSemaphorePermit>>,
    active_kinds: Mutex<HashMap<String, TaskKind>>,
    target_capacity: AtomicUsize,
    slots_to_retire: AtomicUsize,
    notify: Notify,
    notify_permit_released: Notify,

    /// aria2 gid -> download id map (shared with the WS poller).
    pub aria2_gids: Arc<std::sync::RwLock<HashMap<String, String>>>,

    /// gid -> buffered (id_placeholder, outcome) for completions that arrived
    /// before the gid was stored. Drained by `remember_gid`.
    pub pending_completion: Arc<Mutex<HashMap<String, (String, PendingOutcome)>>>,

    /// download id -> spawn payload for aria2 transient-error re-addUri retries.
    aria2_payloads: Mutex<HashMap<String, SpawnPayload>>,

    /// 0-based transient-error strike counter per aria2 download id.
    aria2_retry_strikes: Mutex<HashMap<String, usize>>,

    /// Download ids whose aria2 retry loop must not create another job.
    aria2_retry_cancelled: Mutex<HashSet<String>>,

    spawner: Arc<dyn SidecarSpawner>,
    app_handle: AppHandle<R>,
}

impl QueueManager<tauri::Wry> {
    /// Production constructor. Wired up in lib.rs setup().
    pub fn new(app_handle: AppHandle<tauri::Wry>, capacity: usize) -> Self {
        let spawner: Arc<dyn SidecarSpawner> = Arc::new(ProductionSpawner::new(app_handle.clone()));
        Self::test_new(app_handle, capacity, spawner)
    }
}

impl<R: tauri::Runtime> QueueManager<R> {
    /// Test-only constructor injecting a fake spawner.
    pub fn test_new(
        app_handle: AppHandle<R>,
        capacity: usize,
        spawner: Arc<dyn SidecarSpawner>,
    ) -> Self {
        Self {
            registered_ids: Mutex::new(HashSet::new()),
            pending: Mutex::new(VecDeque::new()),
            semaphore: Arc::new(Semaphore::new(capacity)),
            active_permits: Mutex::new(HashMap::new()),
            active_kinds: Mutex::new(HashMap::new()),
            target_capacity: AtomicUsize::new(capacity),
            slots_to_retire: AtomicUsize::new(0),
            notify: Notify::new(),
            notify_permit_released: Notify::new(),
            aria2_gids: Arc::new(std::sync::RwLock::new(HashMap::new())),
            pending_completion: Arc::new(Mutex::new(HashMap::new())),
            aria2_payloads: Mutex::new(HashMap::new()),
            aria2_retry_strikes: Mutex::new(HashMap::new()),
            aria2_retry_cancelled: Mutex::new(HashSet::new()),
            spawner,
            app_handle,
        }
    }

    /// Current pending order, as id list. Returned by move_in_queue.
    pub async fn pending_order(&self, queue_id: Option<&str>) -> Vec<String> {
        self.pending
            .lock()
            .await
            .iter()
            .filter(|task| queue_id.is_none_or(|queue_id| task.queue_id == queue_id))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Explicitly release a backend registry id (e.g. on un-resumable false paths, removals, or detach).
    pub async fn release_registered_id(&self, id: &str) {
        self.registered_ids.lock().await.remove(id);
    }

    /// Enqueue a task. Checks the centralized `registered_ids` for deduplication.
    pub async fn push(&self, task: QueuedTask) -> Result<(), String> {
        let id = task.id.clone();
        let mut registered = self.registered_ids.lock().await;
        if registered.contains(&id) {
            return Err("Duplicate task".to_string());
        }
        registered.insert(id.clone());
        drop(registered);

        self.pending.lock().await.push_back(task);
        self.emit_state(id, DownloadStatus::Queued);
        self.notify.notify_one();
        Ok(())
    }

    /// Pop the next task, or None if empty.
    pub async fn pop_front(&self) -> Option<QueuedTask> {
        self.pending.lock().await.pop_front()
    }

    /// Acquire a permit from the semaphore (blocks until one is available).
    pub async fn acquire_permit(&self) -> Option<OwnedSemaphorePermit> {
        self.semaphore.clone().acquire_owned().await.ok()
    }

    /// Park an already-acquired permit under `id`.
    pub async fn park_permit(&self, id: &str, permit: OwnedSemaphorePermit) {
        self.active_permits
            .lock()
            .await
            .insert(id.to_string(), permit);
    }

    pub async fn active_kind(&self, id: &str) -> Option<TaskKind> {
        self.active_kinds.lock().await.get(id).cloned()
    }

    /// Ensure an aria2 transfer owns exactly one queue permit. Returns true
    /// when this call acquired and parked the permit, false when one was
    /// already parked.
    pub async fn ensure_aria2_permit(&self, id: &str) -> bool {
        if self.active_permits.lock().await.contains_key(id) {
            return false;
        }

        let permit = match self.acquire_permit().await {
            Some(p) => p,
            None => return false,
        };
        let mut permits = self.active_permits.lock().await;
        if permits.contains_key(id) {
            drop(permits);
            drop(permit);
            return false;
        }
        permits.insert(id.to_string(), permit);
        drop(permits);
        self.active_kinds
            .lock()
            .await
            .insert(id.to_string(), TaskKind::Aria2);
        true
    }

    pub async fn release_permit(&self, id: &str) {
        let removed = self.active_permits.lock().await.remove(id).is_some();
        self.active_kinds.lock().await.remove(id);
        if removed {
            self.notify_permit_released.notify_waiters();
            self.notify.notify_one();
        }
    }

    /// Clear all permits belonging to aria2. Useful when aria2 WS connection drops.
    pub async fn clear_aria2_permits(&self) {
        let ids_to_fail: Vec<String> = {
            let kinds = self.active_kinds.lock().await;
            kinds
                .iter()
                .filter(|(_, kind)| matches!(kind, TaskKind::Aria2))
                .map(|(id, _)| id.clone())
                .collect()
        };

        for id in ids_to_fail {
            self.apply_completion(
                &id,
                PendingOutcome::Error("Aria2 WebSocket connection lost".to_string()),
            )
            .await;
        }
    }

    /// Number of un-acquired permits currently in the semaphore pool.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    fn emit_state(&self, id: impl Into<String>, status: DownloadStatus) {
        use tauri::Emitter;
        let _ = self
            .app_handle
            .emit("download-state", DownloadStateEvent::new(id, status));
    }

    /// Resize the global concurrency limit. Grow adds permits immediately;
    /// shrink records a retirement debt honored lazily by the dispatcher.
    pub fn set_capacity(&self, new_target: usize) {
        let prev_target = self.target_capacity.swap(new_target, Ordering::Relaxed);
        if new_target == prev_target {
            return;
        }
        if new_target > prev_target {
            let mut delta = new_target - prev_target;
            loop {
                let debt = self.slots_to_retire.load(Ordering::Relaxed);
                let to_deduct = std::cmp::min(debt, delta);
                if self
                    .slots_to_retire
                    .compare_exchange_weak(
                        debt,
                        debt - to_deduct,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    delta -= to_deduct;
                    break;
                }
            }
            if delta > 0 {
                self.semaphore.add_permits(delta);
            }
            self.notify.notify_one();
        } else {
            let delta = prev_target - new_target;
            self.slots_to_retire.fetch_add(delta, Ordering::Relaxed);
        }
    }

    /// Test accessor for the retirement debt counter.
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
            let permit_opt = self.semaphore.clone().acquire_owned().await.ok();
            let permit = match permit_opt {
                Some(p) => p,
                None => break, // Semaphore closed, exit dispatcher
            };
            // (3) CAS retirement — never underflows to usize::MAX.
            let mut retired = false;
            let mut debt = self.slots_to_retire.load(Ordering::Relaxed);
            while debt > 0 {
                match self.slots_to_retire.compare_exchange_weak(
                    debt,
                    debt - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        retired = true;
                        break;
                    }
                    Err(actual) => {
                        debt = actual;
                    }
                }
            }
            if retired {
                drop(permit);
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
            Arc::clone(&self).dispatch_one(permit, task).await;
        }
    }

    async fn dispatch_one(self: Arc<Self>, permit: OwnedSemaphorePermit, task: QueuedTask) {
        let id = task.id.clone();
        // Park the permit BEFORE spawning. Uniform parking:
        // aria2's RPC returns instantly, so the permit must outlive the
        // dispatch_one call. Media/Native runners release on exit.
        self.park_permit(&id, permit).await;
        self.active_kinds
            .lock()
            .await
            .insert(id.clone(), task.kind.clone());
        self.emit_state(&id, DownloadStatus::Downloading);

        match task.kind {
            TaskKind::Aria2 => {
                self.aria2_retry_cancelled.lock().await.remove(&id);
                self.aria2_payloads
                    .lock()
                    .await
                    .insert(id.clone(), task.payload.clone());
                self.aria2_retry_strikes.lock().await.remove(&id);
                match self.spawner.add_uri(&id, &task.payload).await {
                    Ok(gid) => self.remember_gid(id.clone(), gid).await,
                    Err(error) => {
                        self.clear_aria2_retry_state(&id).await;
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
                // Native coordinator is event-driven (fire-and-observe). Send
                // Start; completion is handled by the download-complete/
                // download-failed listener in lib.rs setup() which calls
                // release_permit + apply_completion.
                let this = Arc::clone(&self);
                let payload = task.payload.clone();
                let id_for_task = id.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = this.spawner.run_native(&id_for_task, &payload).await {
                        this.emit_failed(&id_for_task, error);
                        this.release_permit(&id_for_task).await;
                    }
                });
            }
        }
    }

    /// Terminal handler for non-aria2 transfers. Emits state and frees the permit.
    /// Does not emit or release anything on intentional MEDIA_RUN_CANCELLED.
    /// Note: `id` is the frontend download UUID, which survives indefinitely as
    /// the terminal state.
    async fn finish_runner(self: Arc<Self>, id: &str, outcome: Result<(), String>) {
        match outcome {
            Ok(()) => {
                self.emit_state(id, DownloadStatus::Completed);
                self.release_registered_id(id).await;
            }
            Err(error) if error == MEDIA_RUN_CANCELLED => {}
            Err(error) => {
                self.emit_failed(id, error);
                self.release_registered_id(id).await;
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
    pub async fn remember_gid(&self, id: String, gid: String) {
        {
            let mut gids = self.aria2_gids.write().unwrap();
            gids.retain(|existing_gid, existing_id| {
                let keep = existing_id != &id || existing_gid == &gid;
                if !keep {
                    log::warn!(
                        "aria2 gid transition [{}]: dropping stale mapping {} before storing {}",
                        id,
                        existing_gid,
                        gid
                    );
                }
                keep
            });
            gids.insert(gid.clone(), id.clone());
        }
        log::info!("aria2 gid transition [{}]: mapped {}", id, gid);
        let buffered = self.pending_completion.lock().await.remove(&gid);
        if let Some((_buf_id, outcome)) = buffered {
            self.apply_completion(&id, outcome).await;
        }
    }

    /// Apply an aria2 completion outcome: release permit + emit state.
    pub async fn apply_completion(&self, id: &str, outcome: PendingOutcome) {
        match outcome {
            PendingOutcome::Complete => {
                self.clear_aria2_retry_state(id).await;
                self.forget_aria2_gid(id).await;
                self.emit_state(id, DownloadStatus::Completed);
                self.release_registered_id(id).await;
            }
            PendingOutcome::Error(error) => {
                self.clear_aria2_retry_state(id).await;
                self.forget_aria2_gid(id).await;
                self.emit_failed(id, error);
                self.release_registered_id(id).await;
            }
        }
        self.release_permit(id).await;
    }

    pub async fn clear_aria2_retry_state(&self, id: &str) {
        self.aria2_payloads.lock().await.remove(id);
        self.aria2_retry_strikes.lock().await.remove(id);
    }

    pub async fn cancel_aria2_retries(&self, id: &str) {
        self.aria2_retry_cancelled
            .lock()
            .await
            .insert(id.to_string());
    }

    pub async fn allow_aria2_retries(&self, id: &str) {
        self.aria2_retry_cancelled.lock().await.remove(id);
    }

    pub fn aria2_gid_for_download(&self, id: &str) -> Option<String> {
        self.aria2_gids
            .read()
            .unwrap()
            .iter()
            .find_map(|(gid, download_id)| (download_id == id).then(|| gid.clone()))
    }

    /// Remove every gid mapping for a download and discard buffered terminal
    /// events for those gids. Returns the most recently encountered gid.
    pub async fn forget_aria2_gid(&self, id: &str) -> Option<String> {
        let removed = {
            let mut gids = self.aria2_gids.write().unwrap();
            let removed: Vec<String> = gids
                .iter()
                .filter(|(_, download_id)| *download_id == id)
                .map(|(gid, _)| gid.clone())
                .collect();
            for gid in &removed {
                gids.remove(gid);
            }
            removed
        };

        if removed.is_empty() {
            return None;
        }

        let mut buffered = self.pending_completion.lock().await;
        for gid in &removed {
            buffered.remove(gid);
            log::info!("aria2 gid transition [{}]: forgot {}", id, gid);
        }
        removed.last().cloned()
    }

    /// Overwrite a stale aria2 gid with the fresh gid minted by a retry
    /// `addUri`. Failing to call this after re-add leaks the semaphore permit.
    pub fn rotate_aria2_gid(&self, id: &str, stale_gid: &str, new_gid: &str) {
        let mut gids = self.aria2_gids.write().unwrap();
        gids.remove(stale_gid);
        gids.insert(new_gid.to_string(), id.to_string());
        log::info!(
            "aria2 gid transition [{}]: rotated {} -> {}",
            id,
            stale_gid,
            new_gid
        );
    }

    async fn wait_permit_released(self: &Arc<Self>, id: &str) {
        loop {
            if !self.active_permits.lock().await.contains_key(id) {
                return;
            }
            let notified = self.notify_permit_released.notified();
            if !self.active_permits.lock().await.contains_key(id) {
                return;
            }
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            }
        }
    }

    /// Intercept transient `onDownloadError` events: backoff, re-issue
    /// `addUri`, and rotate the gid mapping. Permanent errors and exhausted
    /// strikes fall through to a hard `Failed` state.
    async fn handle_aria2_download_error(self: &Arc<Self>, gid: &str, error: String) {
        let id = {
            let gids = self.aria2_gids.read().unwrap();
            gids.get(gid).cloned()
        };
        let id = match id {
            Some(id) => id,
            None => {
                self.pending_completion.lock().await.insert(
                    gid.to_string(),
                    (String::new(), PendingOutcome::Error(error)),
                );
                return;
            }
        };
        if self.aria2_retry_cancelled.lock().await.contains(&id) {
            log::info!(
                "aria2 retry cancellation [{}]: ignoring error for gid {} during removal",
                id,
                gid
            );
            return;
        }

        let strike = {
            let mut strikes = self.aria2_retry_strikes.lock().await;
            let entry = strikes.entry(id.clone()).or_insert(0);
            *entry
        };

        let transient = is_transient_network_error(&error);
        let strikes_left = strike < MAX_RETRIES;
        if !(transient && strikes_left) {
            self.apply_completion(&id, PendingOutcome::Error(error))
                .await;
            return;
        }

        let payload = self.aria2_payloads.lock().await.get(&id).cloned();
        if payload.is_none() {
            self.apply_completion(&id, PendingOutcome::Error(error))
                .await;
            return;
        }
        let payload = payload.unwrap();

        let this = Arc::clone(self);
        let stale_gid = gid.to_string();
        let id_for_task = id.clone();
        let error_for_emit = error.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = backoff_and_emit(
                strike,
                error_for_emit,
                this.wait_permit_released(&id_for_task),
                |reason| {
                    use tauri::Emitter;
                    let _ = this.app_handle.emit(
                        "download-state",
                        DownloadStateEvent::retrying(&id_for_task, reason),
                    );
                },
            )
            .await;

            if outcome == BackoffOutcome::Aborted {
                return;
            }

            if !this.active_permits.lock().await.contains_key(&id_for_task) {
                return;
            }
            if this
                .aria2_retry_cancelled
                .lock()
                .await
                .contains(&id_for_task)
            {
                return;
            }

            match this.spawner.add_uri(&id_for_task, &payload).await {
                Ok(new_gid) => {
                    if this
                        .aria2_retry_cancelled
                        .lock()
                        .await
                        .contains(&id_for_task)
                    {
                        if let Err(error) = this.spawner.remove_uri(&new_gid).await {
                            log::error!(
                                "aria2 retry cancellation [{}]: failed to remove late gid {}: {}",
                                id_for_task,
                                new_gid,
                                error
                            );
                        } else {
                            log::info!(
                                "aria2 retry cancellation [{}]: removed late gid {}",
                                id_for_task,
                                new_gid
                            );
                            return;
                        }
                        this.rotate_aria2_gid(&id_for_task, &stale_gid, &new_gid);
                        log::warn!(
                            "aria2 retry cancellation [{}]: retained late gid {} mapping for remove retry",
                            id_for_task,
                            new_gid
                        );
                        return;
                    }
                    this.aria2_retry_strikes
                        .lock()
                        .await
                        .insert(id_for_task.clone(), strike + 1);
                    this.rotate_aria2_gid(&id_for_task, &stale_gid, &new_gid);
                    this.emit_state(&id_for_task, DownloadStatus::Downloading);
                }
                Err(retry_error) => {
                    this.apply_completion(&id_for_task, PendingOutcome::Error(retry_error))
                        .await;
                }
            }
        });
    }

    /// Entry point for the aria2 WS poller. Resolves gid -> id; if not yet
    /// stored, buffers the outcome for reconciliation by remember_gid.
    pub async fn handle_aria2_event(self: &Arc<Self>, gid: &str, outcome: PendingOutcome) {
        match outcome {
            PendingOutcome::Error(error) => {
                self.handle_aria2_download_error(gid, error).await;
            }
            other => {
                let id_opt = {
                    let gids = self.aria2_gids.read().unwrap();
                    gids.get(gid).cloned()
                };
                match id_opt {
                    Some(id) => {
                        self.apply_completion(&id, other).await;
                    }
                    None => {
                        self.pending_completion
                            .lock()
                            .await
                            .insert(gid.to_string(), (String::new(), other));
                    }
                }
            }
        }
    }

    /// Reorder a pending task up or down. Returns the new pending order.
    /// No-op at boundaries. Does not emit (membership unchanged); the caller
    /// (Tauri command) returns the order to the frontend.
    pub async fn move_in_queue(
        &self,
        id: &str,
        queue_id: &str,
        direction: QueueDirection,
    ) -> Vec<String> {
        let mut pending = self.pending.lock().await;
        let queue_positions = pending
            .iter()
            .enumerate()
            .filter_map(|(index, task)| (task.queue_id == queue_id).then_some(index))
            .collect::<Vec<_>>();
        let queue_pos = queue_positions
            .iter()
            .position(|index| pending[*index].id == id);
        if let Some(queue_pos) = queue_pos {
            let target = match direction {
                QueueDirection::Up => queue_pos.checked_sub(1),
                QueueDirection::Down => {
                    if queue_pos + 1 < queue_positions.len() {
                        Some(queue_pos + 1)
                    } else {
                        None
                    }
                }
            };
            if let Some(target) = target {
                pending.swap(queue_positions[queue_pos], queue_positions[target]);
            }
        }
        pending
            .iter()
            .filter(|task| task.queue_id == queue_id)
            .map(|task| task.id.clone())
            .collect()
    }

    /// Remove a task from pending if present (used by remove_download).
    /// Does NOT release a permit (the caller handles active permits via
    /// release_permit if the task was already dispatched).
    pub async fn remove_from_pending(&self, id: &str) -> bool {
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
    pub async fn enqueue_many(&self, tasks: Vec<QueuedTask>) -> Vec<crate::ipc::EnqueueResult> {
        let mut results = Vec::new();
        let mut registered = self.registered_ids.lock().await;
        let mut pending = self.pending.lock().await;

        for task in tasks {
            let id = task.id.clone();
            let filename = task.payload.filename.clone();
            if registered.contains(&id) {
                results.push(crate::ipc::EnqueueResult {
                    id: id.clone(),
                    success: false,
                    filename: None,
                    error: Some("Duplicate task".to_string()),
                });
                continue;
            }
            registered.insert(id.clone());
            pending.push_back(task);
            self.emit_state(id.clone(), DownloadStatus::Queued);
            results.push(crate::ipc::EnqueueResult {
                id,
                success: true,
                filename: Some(filename),
                error: None,
            });
        }
        drop(pending);
        drop(registered);
        self.notify.notify_one();
        results
    }
}

/// Production spawner that delegates to the real aria2 RPC, yt-dlp, and
/// native coordinator runners.
pub struct ProductionSpawner {
    app_handle: AppHandle<tauri::Wry>,
}

impl ProductionSpawner {
    pub fn new(app_handle: AppHandle<tauri::Wry>) -> Self {
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
        let safe_filename =
            crate::download_ownership::canonical_download_filename(&payload.filename);
        options.insert("out".to_string(), serde_json::json!(safe_filename));
        let conn = payload.connections.unwrap_or(1);
        options.insert("split".to_string(), serde_json::json!(conn.to_string()));
        options.insert(
            "max-connection-per-server".to_string(),
            serde_json::json!(conn.to_string()),
        );
        let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
        options.insert("max-tries".to_string(), serde_json::json!(mt.to_string()));
        options.insert("retry-wait".to_string(), serde_json::json!("2"));
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
        if let Some(prox) = payload.proxy.as_deref().filter(|s| !s.is_empty()) {
            if prox == "none" {
                options.insert("all-proxy".to_string(), serde_json::json!(""));
            } else {
                options.insert("all-proxy".to_string(), serde_json::json!(prox));
            }
        }
        let uris = crate::collect_download_uris(&payload.url, payload.mirrors.as_deref());
        let params = serde_json::json!([uris, options]);

        match crate::rpc_call(
            state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
            &state.aria2_secret,
            "aria2.addUri",
            params,
        )
        .await
        {
            Ok(result) => {
                let gid = result.as_str().unwrap_or("").to_string();
                if gid.is_empty() {
                    Err("aria2.addUri returned an empty gid".to_string())
                } else {
                    log::info!("aria2 addUri [{}]: created gid {}", id, gid);
                    Ok(gid)
                }
            }
            Err(e) => {
                // aria2 unavailable — fall back to native coordinator.
                log::warn!("aria2 addUri failed, falling back to native: {}", e);
                let download_id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
                let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
                let safe_filename =
                    crate::download_ownership::canonical_download_filename(&payload.filename);
                state
                    .download_coordinator
                    .send(crate::download::DownloadCmd::Start(Box::new(
                        crate::download::DownloadPayload {
                            id: download_id,
                            urls: crate::collect_download_uris(
                                &payload.url,
                                payload.mirrors.as_deref(),
                            ),
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
    }

    async fn remove_uri(&self, gid: &str) -> Result<(), String> {
        let state = self.app_handle.state::<crate::AppState>();
        let result = crate::rpc_call(
            state.aria2_port.load(std::sync::atomic::Ordering::Relaxed),
            &state.aria2_secret,
            "aria2.forceRemove",
            serde_json::json!([gid]),
        )
        .await?;
        match result.as_str() {
            Some(returned_gid) if returned_gid == gid => Ok(()),
            Some(returned_gid) => Err(format!(
                "aria2.forceRemove returned unexpected gid {returned_gid}, expected {gid}"
            )),
            None => Err("aria2.forceRemove returned a non-string result".to_string()),
        }
    }

    async fn run_media(&self, id: &str, payload: &SpawnPayload) -> Result<(), String> {
        let state = self.app_handle.state::<crate::AppState>();
        let mut cancel_rx = state
            .download_coordinator
            .register_media(id.to_string())
            .await?;
        let outcome = crate::start_media_download_internal(
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
        .await;
        if outcome.is_ok() {
            if let Ok(path) = crate::download_ownership::expected_primary_path(
                &self.app_handle,
                &payload.destination,
                &payload.filename,
            ) {
                let _ = crate::download_ownership::set_primary_path(&self.app_handle, id, &path);
            }
        }
        let _ = state
            .download_coordinator
            .finish_media(id.to_string())
            .await;
        outcome
    }

    async fn run_native(&self, id: &str, payload: &SpawnPayload) -> Result<(), String> {
        let state = self.app_handle.state::<crate::AppState>();
        let download_id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
        let mt = payload.max_tries.unwrap_or(1).max(1) as u32;
        let resolved_dest = crate::resolve_path(&payload.destination, &self.app_handle);
        let safe_filename =
            crate::download_ownership::canonical_download_filename(&payload.filename);
        let output_path = resolved_dest.join(safe_filename);
        let _ = crate::download_ownership::set_primary_path(&self.app_handle, id, &output_path);
        state
            .download_coordinator
            .send(crate::download::DownloadCmd::Start(Box::new(
                crate::download::DownloadPayload {
                    id: download_id,
                    urls: crate::collect_download_uris(&payload.url, payload.mirrors.as_deref()),
                    output_path,
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
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct EnqueueItem {
    pub id: String,
    pub queue_id: String,
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
    pub fn into_task(self) -> QueuedTask {
        let media = self.is_media.unwrap_or(false);
        let kind = if media {
            TaskKind::Media
        } else {
            TaskKind::Aria2
        };
        let id = self.id.clone();
        QueuedTask {
            id,
            queue_id: self.queue_id,
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
