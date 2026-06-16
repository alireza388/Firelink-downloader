# Download Queue Coordinator — Design Spec

**Date:** 2026-06-16
**Status:** Draft (pending review)
**Owner:** Systems Architect & Queue Concurrency Engineer

## 1. Problem Statement

Firelink currently has **no centralized concurrency authority** for downloads. The frontend Zustand store (`src/store/useDownloadStore.ts`) acts as the sole orchestrator: `processQueue()` counts items in the `downloading` state and greedily dispatches `queued` items up to `maxConcurrentDownloads`.

This has three structural problems:

1. **Three unconverged execution paths** each gate concurrency differently:
   - **aria2 daemon** — manages its own internal `max-concurrent-downloads` via the `aria2.changeGlobalOption` RPC, set by the `set_concurrent_limit` command.
   - **Native HTTP fallback** (`DownloadCoordinator` in `download.rs`) — has *no* concurrency limit; it spawns a task per `DownloadCmd::Start`.
   - **yt-dlp media** (`start_media_download`) — gated by a hardcoded `media_semaphore: Arc<Semaphore>` sized to 3.

   These can drift out of sync and over-subscribe system resources.

2. **No single source of truth** for "how many slots are truly in use." The frontend *infers* this from status fields, racing against asynchronous backend emit events.

3. **No reordering.** Queue order is implicit in the `downloads` array index; users cannot prioritize pending tasks.

## 2. Goals & Non-Goals

### Goals
- Introduce a backend `QueueManager` as the **sole gatekeeper** of download dispatch across all three execution paths (aria2, native HTTP, yt-dlp media).
- Enforce a configurable `max_concurrent_tasks` limit via a single `tokio::sync::Semaphore`.
- Guarantee pending tasks sit in a strict `Queued` state until a permit is secured — never auto-spawn sidecars on enqueue.
- Release concurrency slots immediately on pause, regardless of sidecar teardown latency.
- Support manual reordering (Move Up / Move Down) of pending tasks.
- Provide clear UI affordances for the `Queued` state.

### Non-Goals
- Per-queue concurrency budgets (single global pool only).
- Backend-owned persistence of queue contents (frontend remains the persistence owner).
- Changes to the scheduler feature (`scheduler.rs` — time-of-day start/stop) beyond adapting to the new dispatch API.
- Network/CPU-aware dynamic concurrency tuning.

## 3. Key Decisions (from brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Backend ownership | **Full** — QueueManager owns queue + permits + dispatch + transitions | Single source of truth; eliminates frontend/backend races |
| Aria2 path | **Routed through QueueManager permits** | Unifies all three paths under one semaphore |
| Permit pool | **Single global** `Arc<Semaphore>` | "3 concurrent" means 3 total, regardless of type |
| Queue scope | **Single global** FIFO vector | Matches existing single MAIN_QUEUE_ID usage |
| Persistence | **Frontend re-enqueues on startup** | Store remains the single persistence layer |

## 4. Architecture Overview

### Core Invariant
> **No sidecar process (aria2 RPC add, yt-dlp, or native HTTP) is spawned until a semaphore permit is secured.** The frontend never calls `start_download` / `start_media_download` directly — it calls `enqueue_download`, and the backend decides *when* to actually start.

### Data Flow

```
Frontend: addDownload(item)
   │  invokes enqueue_download(payload)
   ▼
Backend QueueManager.push(task)
   │  appends to pending VecDeque
   │  notify.notify_one()
   │  emits download-state { id, status: "queued" }
   ▼
Dispatcher loop (parks when idle, woken by Notify)
   │  semaphore.acquire_owned().await   ← blocks until slot free
   │  CAS-check retirement debt (never underflows)
   │  pops head of pending VecDeque
   │  parks permit in active_permits[id]
   │  emits download-state { id, status: "downloading" }
   │  spawns sidecar (aria2/yt-dlp/native) — or, for aria2, parks permit + issues RPC
   ▼
Sidecar completes/pauses/fails
   │  release_permit(id) → remove from active_permits → permit drops → slot freed
   │  notify.notify_one() → dispatcher wakes, claims next queued item
   ▼
Emits download-state { completed | failed | paused }
```

### What gets removed / changed
- **Removed:** frontend `processQueue()` dispatch logic.
- **Removed:** the `_dispatched` flag on `DownloadItem` (backend tracks dispatch).
- **Removed:** direct `invoke('start_download' / 'start_media_download')` from the frontend store.
- **Removed:** the standalone hardcoded `media_semaphore` (folded into the global queue).
- **Changed:** `set_concurrent_limit` resizes the backend semaphore. aria2's own `max-concurrent-downloads` is set to a fixed high ceiling of **100** at daemon startup (see §5.6) so it never double-gates — the backend semaphore is the sole concurrency authority.
- **Kept (internal):** `start_download` / `start_media_download` become `pub(crate)` functions the QueueManager calls.

## 5. QueueManager Internals

### Module layout
New file: `src-tauri/src/queue.rs`. The `QueueManager` is a managed Tauri state field on `AppState`, accessible via `tauri::State<'_, AppState>`.

### Struct

```rust
pub struct QueueManager {
    /// Pending tasks, FIFO. Index 0 is dispatched next.
    pending: Mutex<VecDeque<QueuedTask>>,

    /// The single global concurrency gate.
    semaphore: Arc<Semaphore>,

    /// Held permits keyed by download id (uniform parking — see §5.3).
    active_permits: Mutex<HashMap<String, OwnedSemaphorePermit>>,

    /// Runtime-resizable capacity: the user's desired ceiling.
    target_capacity: AtomicUsize,

    /// Retirement debt: number of returning permits to absorb (see §5.4).
    slots_to_retire: AtomicUsize,

    /// Wake signal: pokes the dispatcher when pending changes or a slot frees.
    notify: Notify,

    /// Shared aria2 state needed to spawn aria2 tasks.
    aria2: Aria2Handle,
}

struct QueuedTask {
    id: String,
    kind: TaskKind,            // Aria2 | Media | Native
    payload: SpawnPayload,     // all args needed to start the sidecar
}
```

`QueueManager` is wrapped in `Arc` and cloned into the dispatcher task. It is **not** behind an outer `Mutex` — only its interior fields are locked individually, so the dispatcher never holds a lock while awaiting the semaphore (which would deadlock).

### Worker model: one dispatcher, many runners
- **One** long-lived dispatcher task (`run_dispatcher`), spawned in `setup()`. Owns the acquire-and-pop loop.
- **Per-task** runner futures (spawned via `tauri::async_runtime::spawn` inside `dispatch_one`). For Media/Native these run to completion holding the permit; for Aria2 they issue the RPC and exit, leaving the permit parked.

This avoids the "N worker tasks each racing on the queue" pattern (head-of-line blocking, wasted wakeups). One dispatcher = one deterministic consumer.

### 5.1 Dispatcher loop (final, with all three correctness fixes)

Combines: idle-park (no busy-spin), CAS retirement (no underflow), re-pop guard (race safety).

```rust
async fn run_dispatcher(self: Arc<Self>) {
    loop {
        // (1) Idle-park: avoid busy-spin when queue is empty.
        if self.pending.lock().is_empty() {
            self.notify.notified().await;
            continue;
        }
        // (2) Acquire a concurrency slot.
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        // (3) CAS retirement — never underflows to usize::MAX.
        let mut retired = false;
        loop {
            let debt = self.slots_to_retire.load(Relaxed);
            if debt == 0 { break; }
            match self.slots_to_retire.compare_exchange_weak(debt, debt - 1, Relaxed, Relaxed) {
                Ok(_) => { drop(permit); retired = true; break; }
                Err(_actual) => { /* retry with updated debt */ }
            }
        }
        if retired { continue; }
        // (4) Re-pop under lock — guards against racing removals between
        //     waking from Notify and acquiring the permit.
        let task = match self.pending.lock().pop_front() {
            Some(t) => t,
            None => { drop(permit); continue; }
        };
        self.dispatch_one(permit, task);
    }
}
```

**The three bugs this design discovered and fixes (each gets a regression test, §8):**
- **Idle CPU spin** — `acquire_owned` immediately resucceeds after returning a permit to an empty pool. Fixed by peek→park→`Notify`.
- **`fetch_sub` underflow → `usize::MAX` → permanent deadlock** — `fetch_sub` is unconditional. Fixed by CAS loop.
- **Racing removal** — pending can drain between `Notify` wake and acquire. Fixed by re-pop guard.

### 5.2 Wakeup invariant (hard rule)
> Every code path that adds to `pending` OR frees a permit OR grows capacity MUST call `self.notify.notify_one()`.

Applies to: `enqueue_download`, `enqueue_many`, `resume_download`, `release_permit`, `set_capacity` (grow branch).

### 5.3 Uniform permit parking (aria2 trap + immediate pause release)

**Problem (aria2):** the `aria2.addUri` RPC returns *immediately* with a GID; the download runs asynchronously inside the daemon. If we held the permit in the task that issued the RPC, it would drop instantly and the slot would be "free" while the download was actually running.

**Solution — uniform parking for ALL task kinds:** every acquired permit is parked in `active_permits: HashMap<id, OwnedSemaphorePermit>` before the sidecar spawns. This also gives us the "pause releases the slot immediately" property for free.

```rust
async fn dispatch_one(&self, permit: OwnedSemaphorePermit, task: QueuedTask) {
    let id = task.id.clone();
    // Park the permit BEFORE spawning, keyed by id.
    self.active_permits.lock().insert(id.clone(), permit);
    self.emit_state(&id, DownloadStatus::Downloading, None);

    match task.kind {
        TaskKind::Media => {
            // Runner task owns the cleanup; permit already parked.
            tauri::async_runtime::spawn(self.clone().run_media(task, id));
        }
        TaskKind::Native => {
            tauri::async_runtime::spawn(self.clone().run_native(task, id));
        }
        TaskKind::Aria2 => {
            // Issue the RPC (returns immediately with a gid). Permit stays parked.
            // This task ends here; release happens via WS poller / pause / remove.
            let gid = self.aria2.add_uri(task).await;
            self.remember_gid(id.clone(), gid);   // see §5.5 for race handling
        }
    }
}
```

`release_permit` — **idempotent**, called from every completion/pause/remove path:

```rust
fn release_permit(&self, id: &str) {
    let removed = self.active_permits.lock().remove(id).is_some();
    if removed {
        self.notify.notify_one();   // wake dispatcher: a slot opened
    }
}
```

**Permit release points by task kind:**

| Kind | Release point |
|---|---|
| Media / Native | Runner task exit (any path) calls `release_permit(id)`. If pause already removed it, the call is a harmless no-op. |
| Aria2 | WebSocket poller (`onDownloadComplete` / `onDownloadError`), `pause_download`, `remove_download`, app shutdown. |

**Pause guarantee:** `pause_download` calls `release_permit(id)` *first* (slot free, dispatcher can immediately grab the next queued item), *then* sends the kill/cancel signal to the sidecar. The dying sidecar's eventual exit calls `release_permit` again → no-op.

### 5.4 Resizing the semaphore (grow AND shrink)

`Semaphore::add_permits(n)` grows cleanly. **Shrinking has no primitive** — you cannot forcibly revoke an in-flight permit. Safe approach: **lazy retirement debt.**

```rust
fn set_capacity(&self, new_target: usize) {
    let prev_target = self.target_capacity.swap(new_target, Relaxed);
    if new_target == prev_target { return; }
    if new_target > prev_target {
        // GROW: add the delta. Safe, immediate. Wake dispatcher in case
        // items are waiting.
        let delta = new_target - prev_target;
        self.semaphore.add_permits(delta);
        self.notify.notify_one();
    } else {
        // SHRINK: record debt. Retire permits lazily as tasks finish.
        let delta = prev_target - new_target;
        self.slots_to_retire.fetch_add(delta, Relaxed);
    }
}
```

**Why this is safe for running streams:** we never touch in-flight permits. Shrinking from 5→3 with 5 active downloads ensures that as each of the 5 finishes, *two* of those returning slots get absorbed instead of re-dispatched. Active downloads are never killed or interrupted. The shrink "takes effect" over the next `delta` completions — the only sane behavior.

**Convergence invariant:** `slots_to_retire.load() ≤ current_effective_capacity − target_capacity`, never negative (enforced by the CAS loop), never wraps to `usize::MAX`.

**Subtle dispatcher detail:** if the dispatcher acquires a permit to retire it but there's no pending work, it still `drop(permit)`s (returns it to the pool) and the `slots_to_retire` was already decremented, so the *next* acquire retires again until debt is exhausted. Converges correctly after `delta` total retirements. The idle-park check (step 1) prevents busy-spinning during this.

### 5.5 aria2 GID lookup race

**The race:** `add_uri` returns a gid that completes *before* we store `id → gid` in `aria2_gids`. The WebSocket `onDownloadComplete` event arrives with an unknown gid and gets dropped, leaking the permit.

**Fix — buffer + reconcile:**

1. **Order of operations in `dispatch_one` for Aria2:**
   1. Park permit in `active_permits[id]` (BEFORE the RPC).
   2. Call `add_uri` → get gid.
   3. Store `aria2_gids[id] = gid`.
   4. Check `pending_completion`: if the gid is present (completion arrived between steps 2 and 3), process it now (release permit, emit complete).

2. **WebSocket handler** (existing poller in `lib.rs:1570`, extended):
   - On `onDownloadComplete` / `onDownloadError` for a gid:
     - If `aria2_gids` has `id` for that gid → `release_permit(&id)` + emit state.
     - Else → insert gid into `pending_completion: Mutex<HashSet<Gid>>`. It will be reconciled when `remember_gid` runs step 4 above.

3. **`pending_completion`** is `Mutex<HashMap<String, PendingOutcome>>` where `PendingOutcome = { id: String, kind: Complete | Error(String) }`, keyed by gid, living on `QueueManager` (or `Aria2Handle`). On lookup it is drained and the stored `id` + `kind` drive `release_permit` + the correct state emit. This is why a buffered entry carries the outcome, not just the gid — the `onDownloadComplete` vs. `onDownloadError` distinction must survive the race.

This closes the only ordering gap; permits are never leaked by a completion that outran the gid store.

### 5.6 aria2 daemon configuration
See §6.1 — aria2's own concurrency limit is pinned to a high ceiling at startup so the backend semaphore is the sole gate. No runtime `aria2.changeGlobalOption` for `max-concurrent-downloads` is issued by `set_concurrent_limit` anymore.

## 6. State-Transition Protocol

### Frontend becomes reactive, not imperative
The frontend **requests** ("enqueue", "pause", "move up") and the backend **reports** state transitions via events. The store never guesses status.

### Unified event: `download-state`

```rust
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadStateEvent {
    pub id: String,
    pub status: DownloadStatus,   // queued | downloading | paused | completed | failed
    pub error: Option<String>,
}
```

Emitted on every transition. `download-progress` stays as-is but **loses** its status-mutating side effect (only updates fraction/speed/eta/size). `download-complete` / `download-failed` events remain for notification wiring; the store's source of truth for *status* is `download-state`.

### Transition table (backend-owned)

| From → To | Trigger | Permit action |
|---|---|---|
| `(new)` → `Queued` | `enqueue_download` | — (no permit yet) |
| `Queued` → `Downloading` | dispatcher acquires permit, spawns sidecar | permit acquired + parked |
| `Downloading` → `Paused` | `pause_download` | `release_permit(id)` **first**, then kill sidecar |
| `Downloading` → `Completed` | sidecar exit 0 / aria2 `onDownloadComplete` | runner exit / WS poller → `release_permit(id)` |
| `Downloading` → `Failed` | sidecar nonzero / aria2 `onDownloadError` | runner exit / WS poller → `release_permit(id)` |
| `Paused` → `Queued` | `resume_download` (re-enqueues) | — (re-enters queue) |
| `Completed/Failed` → `Queued` | `redownload` → `enqueue_download` | — |
| `Queued` → `(removed)` | `remove_download` (pops from pending VecDeque) | — |

### Backend command surface

| Command | Signature | Purpose |
|---|---|---|
| `enqueue_download` | `(payload: SpawnPayload) -> Result<String, AppError>` | Push one task; returns id; emits `download-state{queued}` |
| `enqueue_many` | `(ids: Vec<String>) -> Result<(), AppError>` | Re-enqueue existing persisted items (startup, start-all); emits `download-state{queued}` per id |
| `pause_download` | `(id: String) -> Result<(), AppError>` | Release permit + kill sidecar; emit `download-state{paused}` |
| `resume_download` | `(id: String) -> Result<(), AppError>` | Re-enqueue a paused item; emit `download-state{queued}` |
| `move_in_queue` | `(id: String, direction: QueueDirection) -> Result<Vec<String>, AppError>` | Reorder pending VecDeque; **returns new pending order** |
| `remove_from_queue` | `(id: String) -> Result<(), AppError>` | Pop from pending if present (called by `remove_download`) |
| `set_concurrent_limit` | `(limit: usize) -> Result<(), AppError>` | Grow semaphore / set retirement debt |

`QueueDirection` is `#[derive(Serialize, Deserialize, TS)] enum { Up, Down }`.

### 6.1 aria2 daemon: fixed high ceiling
At daemon startup (`lib.rs` setup), aria2's `--max-concurrent-downloads` is pinned to **100** (or omitted, since aria2's default is 5 — we explicitly raise it). The user-facing `maxConcurrentDownloads` setting *only* drives the backend `QueueManager` semaphore via `set_concurrent_limit`. This removes aria2 as a second concurrency gate entirely: the backend semaphore is the sole authority, and aria2 will accept whatever `addUri` calls the dispatcher issues (capped at `maxConcurrentDownloads` by the permit pool).

## 7. Frontend Changes

### 7.1 `useDownloadStore.ts`

**Removed:**
- `processQueue()` (and the `isProcessingQueue` guard).
- The `_dispatched` field on `DownloadItem`.
- Status-mutation triggers inside `updateDownload` (no more `if (updates.status ...) processQueue()`).
- Direct `invoke('start_download' / 'start_media_download')` calls.

**Added state slice:**
```typescript
pendingOrder: string[];   // authoritative pending order; maintained ONLY by the download-state listener + move_in_queue
```

**Authority rule for `pendingOrder`:** the `download-state` listener (§7.2) is the single source of truth for membership — it appends on `queued` and removes on any other status. Store actions do **not** optimistically mutate `pendingOrder` on enqueue/resume/startQueue (they would race the listener and double-add). The only action that writes `pendingOrder` directly is `moveUp`/`moveDown`, because `move_in_queue` reorders *within* the existing set and returns the full new order — that is not a membership change, so no listener event fires for it.

**Changed actions (reactive):**
```typescript
addDownload: (item) => {
  set((state) => ({ downloads: [...state.downloads, item] }));
  // Backend emits download-state{queued}; the listener appends to pendingOrder.
  // No optimistic pendingOrder mutation here.
  invoke('enqueue_download', { payload: buildSpawnPayload(item) }).catch(console.error);
},

startQueue: async (queueId) => {
  const ids = get().downloads
    .filter(d => d.queueId === queueId && ['paused','failed'].includes(d.status))
    .map(d => d.id);
  if (ids.length === 0) return 0;
  // Backend emits download-state{queued} per id; the listener appends each.
  await invoke('enqueue_many', { ids });
  return ids.length;
},

pauseQueue: async (queueId) => {
  const ids = get().downloads
    .filter(d => d.queueId === queueId && d.status === 'downloading')
    .map(d => d.id);
  if (ids.length === 0) return 0;
  await Promise.all(ids.map(id => invoke('pause_download', { id })));
  return ids.length;
},

pauseDownload:  (id) => invoke('pause_download', { id }),
resumeDownload: (id) => invoke('resume_download', { id }),
// moveUp/moveDown are the ONLY actions that write pendingOrder directly:
moveUp:   (id) => invoke('move_in_queue', { id, direction: 'Up' }).then(order => set({ pendingOrder: order })),
moveDown: (id) => invoke('move_in_queue', { id, direction: 'Down' }).then(order => set({ pendingOrder: order })),
```

**`updateDownload`** keeps syncing system integrations (dock badge, prevent-sleep) on status change but no longer calls `processQueue`.

### 7.2 `downloadStore.ts` — new event listener

```typescript
import type { DownloadStateEvent } from '../bindings/DownloadStateEvent';

listen<DownloadStateEvent>('download-state', (event) => {
  const { id, status, error } = event.payload;
  const mainStore = useDownloadStore.getState();
  mainStore.updateDownload(id, { status, ...(error ? { error } : {}) });
  // pendingOrder is authoritative here: append on queued, remove otherwise.
  // (This is the ONLY place membership changes — see §7.1 authority rule.)
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
```

The existing `download-progress` listener is simplified: it updates `useDownloadProgressStore` only; it no longer flips `queued → downloading` (the backend's `download-state` does that).

### 7.3 Startup (`initDB`)
After loading persisted downloads, any item whose persisted status is `downloading` is treated as **interrupted** (the sidecar died with the app). The frontend resets it to `queued` locally, then bulk-enqueues all `queued` items via `enqueue_many`. The backend reconstructs its pending VecDeque from this. This is the "frontend re-enqueues on startup" decision.

### 7.4 `DownloadItem.tsx` — Queued UI

Three layered cues so "pending a slot" reads as *alive and waiting*, not *frozen or dead*:

1. **Grayscale + dimmed progress fill.** New CSS class `download-progress-fill.queued`: `filter: grayscale(1) opacity(0.45)` on the fill; the track gets a subtle animated striped background (`linear-gradient` + `background-position` keyframe).
2. **Clock icon** in the status text (replaces bare "Queued" word). `Clock` from `lucide-react` (already a dependency). Paired with "Queued" for accessibility.
3. **Position badge** — `#N` ordinal chip from `pendingOrder.indexOf(id) + 1`.

```tsx
const pendingOrder = useDownloadStore(s => s.pendingOrder);
const queuePosition = pendingOrder.indexOf(download.id) + 1;
const queueLength = pendingOrder.length;

// In the status cell:
{download.status === 'queued' ? (
  <span className="download-status download-status-queued">
    <Clock size={12} className="animate-pulse-slow" />
    <span>Queued</span>
    <span className="queue-position-badge">#{queuePosition}</span>
  </span>
) : download.status === 'completed' ? (
  /* existing completed branch */
) : (
  /* existing progress branch — progress fill gets 'queued' class only when paused */
)}
```

**Move Up / Move Down buttons** in the existing hover action cluster (`app-icon-button h-7 w-7` pattern). `ChevronUp` / `ChevronDown` from `lucide-react`. Disabled at queue boundaries:

```tsx
{download.status === 'queued' && (
  <>
    <button
      onClick={() => moveUp(download.id)}
      disabled={queuePosition === 1}
      className="app-icon-button h-7 w-7"
      title="Move up"
    ><ChevronUp size={14} /></button>
    <button
      onClick={() => moveDown(download.id)}
      disabled={queuePosition === queueLength}
      className="app-icon-button h-7 w-7"
      title="Move down"
    ><ChevronDown size={14} /></button>
  </>
)}
```

**`pendingOrder` sync (the UI freeze fix):** because the frontend array stays in submission order, the `pendingOrder: string[]` slice — returned from `move_in_queue` and maintained on enqueue/resume/dispatch — is what makes the `#N` badges update instantly on click. Without it, badges would stay frozen until the item dispatches.

### 7.5 What does NOT change
- `DownloadTable.tsx` row layout / grid template.
- Progress bar mechanics for `downloading` (transient `useDownloadProgressStore` subscription).
- Completed/Failed/Paused visuals.
- Tray `pause-all` / `resume-all` (routes through `pauseQueue`/`startQueue` → backend commands).

## 8. Testing Strategy

The design surfaced four concrete correctness bugs during review. Each gets a regression test that would have caught it. Tests live in `src-tauri/tests/queue_manager.rs`, mirroring `tests/download_engine.rs` style (headless `QueueManager` with a fake sidecar spawner, `tokio::time::timeout`).

### Rust integration tests

| Test | Catches | Assertion |
|---|---|---|
| `dispatcher_parks_when_idle` | Busy-loop bug | No `acquire_owned` calls over a 200ms idle window (counting wrapper around the semaphore) |
| `cas_retirement_never_underflows` | `usize::MAX` deadlock | Repeated `set_capacity(3)` → `set_capacity(1)` while idle; `slots_to_retire` stays in `[0, 2]`, never wraps |
| `shrink_converges_to_target` | Shrink correctness | Capacity 3→5→2 with tasks completing; active count settles at exactly 2 |
| `grow_releases_immediately` | Grow path | Capacity 2→4 while 2 tasks wait in pending; both dispatch within timeout |
| `aria2_permit_survives_rpc_return` | Permit trap | Fake `add_uri` returns instantly; permit remains parked in `active_permits`; slot NOT freed until `release_permit` |
| `release_permit_is_idempotent` | Double-release | Call `release_permit(id)` twice; semaphore available count increments only once |
| `gid_completion_before_store` | GID race | Completion event for un-stored gid buffers in `pending_completion`; reconciles on `remember_gid`, releases exactly one permit |
| `pause_releases_slot_before_sidecar_dies` | Immediate-release guarantee | Pause a task whose fake sidecar hangs; a queued item dispatches *before* the sidecar exit is observed |
| `move_up_down_reorders_pending` | Reordering | Push A,B,C; move C up; dispatch order is A,C,B |
| `notify_fires_on_enqueue_and_release` | Wakeup invariant | Dispatcher wakes from idle-park within 50ms of `enqueue_download` / `release_permit` |

### Frontend tests (if Vitest harness exists)
Minimal, since the store is now thin:
- `download-state` event drives status mutation; `processQueue` is gone and not invoked.
- `move_in_queue` return value updates `pendingOrder`; `queuePosition` recomputes.

### Manual smoke test (documented)
1. Add 5 downloads with `maxConcurrentDownloads=3`; observe 3 dispatch + 2 show Queued with clock icon + position badge.
2. Shrink to 1 mid-flight; observe graceful convergence without killing active downloads.
3. Pause one; watch a queued item claim the slot instantly (before the paused sidecar fully exits).
4. Move Up/Down on a queued item; observe `#N` badges update instantly.
5. Restart the app mid-download; observe interrupted `downloading` items reset to `queued` and resume via `enqueue_many`.

## 9. Migration / Rollout

This is an internal refactor with no persisted-schema change (the `download_queue` store shape is unchanged; `_dispatched` was never persisted). Rollout is atomic in a single PR — the backend QueueManager and the reactive frontend must ship together (a reactive frontend against the old imperative backend, or vice versa, would break).

**Commit sequencing within the PR** (for reviewability):
1. Add `queue.rs` with `QueueManager` + dispatcher + tests (backend self-contained, not yet wired).
2. Wire `QueueManager` into `AppState`; convert `start_download` / `start_media_download` to `pub(crate)`; add `enqueue_*` / `pause` / `resume` / `move_in_queue` / `remove_from_queue` commands.
3. Extend the aria2 WebSocket poller with `release_permit` + `pending_completion` reconciliation.
4. Frontend: remove `processQueue`, add `pendingOrder` slice, add `download-state` listener.
5. Frontend: `DownloadItem.tsx` queued visuals + Move Up/Down buttons + CSS.

## 10. Open Questions

None. All architectural forks were resolved during brainstorming (§3). Implementation details (exact `SpawnPayload` shape, CSS class names) are deferred to the implementation plan.
