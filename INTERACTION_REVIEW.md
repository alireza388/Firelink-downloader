# Firelink — UI Interaction Inventory

A read-only audit of every user-facing action in Firelink, mapping each to the
component/handler, Zustand store action, IPC command, Rust function, expected
behavior, and the likely validation method. **No code was changed.**

Legend:
- ✅ = wired end-to-end (UI → store → IPC → Rust)
- ⚠️ = present in UI/store but with a gap, mismatch, or risk (see notes)
- ❌ = declared/wired in one layer but missing in another (dead path)

---

## 1. Main Window

### 1.1 Title bar / Toolbar (`DownloadTable.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| M1 | Add Download (＋) | `DownloadTable` → `toggleAddModal(true)` | `useDownloadStore.toggleAddModal` | — | — | Opens Add Downloads modal | None |
| M2 | Resume All (▶) | `DownloadTable` maps over `paused` items → `handleResume` | `useDownloadStore.resumeDownload` (per item) | `resume_download` | `lib.rs:2211 resume_download` | Resumes every paused download in the current filter | Per-item status check |
| M3 | Pause All (⏸) | `DownloadTable` maps over `downloading` items → `handlePause` | — (direct) | `pause_download` | `lib.rs:2137 pause_download` | Pauses every downloading item in the current filter | Per-item aria2 status |
| M4 | Show Sidebar (PanelLeft, when hidden) | `DownloadTable` → `toggleSidebar` | `useSettingsStore.toggleSidebar` | — | — | Reveals sidebar; persisted | None |
| M5 | Drag window | `WindowDragRegion` / `data-tauri-drag-region` divs → `getCurrentWindow().startDragging()` | — | core window | Tauri core | Moves window | Native |
| M6 | Sidebar resize handle | `App.startSidebarResize` (pointer drag) | `setSidebarWidth` (local) → `localStorage` | — | — | Resizes sidebar 190–260px | Clamped in handler |

⚠️ **M2/M3**: "Resume All"/"Pause All" only act on items in the **current sidebar
filter** (e.g. active vs. all). A user on the "Active" filter pausing all won't
touch paused items elsewhere — could surprise users who read the button as global.
Contrast with the tray's true global Pause/Resume All (`downloadStore.ts:79`).

### 1.2 Download row actions (`DownloadItem.tsx`, `DownloadTable.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| M7 | Move Up / Move Down (queued) | `DownloadItem` → `moveInQueue(id,'up'\|'down')` | `useDownloadStore.moveInQueue` | `move_in_queue` | `lib.rs:2613 move_in_queue` | Reorders pending queue; disabled at bounds | `queueIndex === 0` / `length-1` |
| M8 | Pause (row, active) | `DownloadItem.handlePause` | — | `pause_download` | `pause_download` | Pauses aria2/native/media task | aria2 status check |
| M9 | Resume (row, paused) | `DownloadItem.handleResume` | `resumeDownload` | `resume_download` | `resume_download` | Resumes; re-enqueues if no gid | gid presence |
| M10 | Options menu (⋮) / right-click | `DownloadItem` → `setContextMenu` | — | — | — | Opens context menu | — |
| M11 | Double-click row | `handleDownloadDoubleClick` | — | `open_downloaded_file` | `commands.rs:27 open_downloaded_file` | Completed → open file; else → Properties | `item.status === 'completed'` |

### 1.3 Row context menu (`DownloadTable.tsx`, floating menu)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| C1 | Open | `openDownloadFile` | — | `open_downloaded_file` | `open_downloaded_file` | Opens completed file | `status==='completed'`; path authorization |
| C2 | Show in Finder | `revealDownloadFile` | — | `reveal_in_file_manager` | `commands.rs:6 reveal_in_file_manager` | Reveals file/part in Finder | `status==='completed'`; path authorization |
| C3 | Pause | `handlePause` | — | `pause_download` | `pause_download` | Pauses | status in `{downloading,queued,retrying}` |
| C4 | Resume | `handleResume` | `resumeDownload` | `resume_download` | `resume_download` | Resumes | status in `{paused,failed,retrying}` |
| C5 | Redownload | `redownload(contextItem.id)` | `useDownloadStore.redownload` | `enqueue_download` (via `dispatchItem`) | `enqueue_download` | Creates a new queued copy | status in `{completed,failed,paused}` |
| C6 | Copy Address | `navigator.clipboard.writeText(url)` | — | — | — | Copies URL | try/catch toast |
| C7 | Copy File Path | clipboard write | — | — | — | Copies resolved path | `status==='completed'`; filename present |
| C8 | Remove | `handleDelete` → `openDeleteModal(id)` | `useDownloadStore.openDeleteModal` | — | — | Opens Delete modal | — |
| C9 | Properties | `openProperties` → `setSelectedPropertiesDownloadId` | `useDownloadStore.setSelectedPropertiesDownloadId` | — | — | Opens Properties modal | — |

### 1.4 Column resize

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| M12 | Resize column handle | `DownloadTable.startColumnResize` | `setColumnWidths` (local) | — | — | Resizes table columns | per-column minimums |

### 1.5 Sidebar (`Sidebar.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| S1 | Library filters (All/Active/Completed/Unfinished) | `NavItem` → `onSelectFilter` | `App.setFilter` + `setActiveView('downloads')` | — | — | Filters download list | — |
| S2 | Folder filters (Musics/Movies/…/Other) | `NavItem` | as above | — | — | Filters by `category` | — |
| S3 | Queue item select | `QueueItem` → `onSelectFilter('queue:<id>')` | as above | — | — | Filters by `queueId` | — |
| S4 | Add new queue | "Add new queue" button → input → `handleAddQueueSubmit` | `useDownloadStore.addQueue` | — | — | Adds queue (persisted to store.bin) | name trimmed non-empty |
| S5 | Queue rename | context → Rename → `handleRenameQueueSubmit` | `useDownloadStore.renameQueue` | — | — | Renames queue | name trimmed non-empty |
| S6 | Queue delete | context → Delete | `useDownloadStore.removeQueue` | — | — | Deletes queue; reassigns downloads to Main | `id !== MAIN_QUEUE_ID` |
| S7 | Start Queue | context → Start Queue | `useDownloadStore.startQueue` | `enqueue_download`/`resume_download` | dispatchItem | Dispatches/resumes queue's runnable items | queued/paused/failed |
| S8 | Pause Queue | context → Pause Queue | `useDownloadStore.pauseQueue` | `pause_download` | `pause_download` | Pauses all downloading in queue | status `downloading` |
| S9 | Tools: Scheduler | `ToolItem view='scheduler'` | `setActiveView('scheduler')` | — | — | Shows Scheduler view | — |
| S10 | Tools: Speed Limiter | `ToolItem view='speedLimiter'` | `setActiveView('speedLimiter')` | — | — | Shows Speed Limiter view | — |
| S11 | Tools: Diagnostics | `ToolItem view='diagnostics'` | `setActiveView('diagnostics')` | — | — | Shows Diagnostics console | — |
| S12 | Settings | footer button | `setActiveView('settings')` | — | — | Shows Settings view | — |
| S13 | Hide Sidebar | `toggleSidebar` | `useSettingsStore.toggleSidebar` | — | — | Collapses sidebar | — |

### 1.6 Global handlers (App-level `useEffect` listeners)

| # | Action | Trigger | Handler | Store/IPC | Rust fn | Expected behavior | Validation |
|---|--------|---------|---------|-----------|---------|-------------------|------------|
| G1 | Paste URLs | window `paste` (non-input) | `extractValidDownloadUrls` → `openAddModalWithUrls` | store | — | Opens Add modal with pasted URLs | URL scheme allowlist |
| G2 | Deep link | `deep-link-add-download` event | `openAddModalWithUrls(payload)` | store | `dispatch_deep_links` → `parse_firelink_urls` | Opens Add modal | scheme/host/length caps |
| G3 | Extension add | `extension-add-download` event | `handleExtensionDownload` | store | extension_server | Opens Add modal | dedupe URLs |
| G4 | Extension queued batch | `extension-downloads-queued` event | merges into `downloads`/`pendingOrder` | setState | extension_server | Appends new items | id dedupe |
| G5 | Schedule trigger | `schedule-trigger` event ('start'/'stop') | `startQueue`/`pauseQueue` + `setSchedulerRunning` | store+IPC | scheduler.rs | Starts/stops main queue | — |
| G6 | Post-queue system action | scheduler end + `postQueueAction !== 'none'` | `invoke('perform_system_action')` | — | `lib.rs:2555 perform_system_action` | Sleep/restart/shutdown | automation permission |
| G7 | Notification | `download-state` terminal | `sendNotification` | — | — | OS notification | permission; `showNotifications` |

---

## 2. Add Download Window (`AddDownloadsModal.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| A1 | Paste/edit URLs textarea | `setUrls` (local) → debounce parse | — | `fetch_metadata` / `fetch_media_metadata` (via `fetchMediaMetadataDeduped`) | `lib.rs:616 fetch_metadata`, `lib.rs:784 fetch_media_metadata` | Resolves filename/size/formats | `new URL()`; isMediaUrl |
| A2 | Refresh Metadata | `setMetadataRefreshNonce(n+1)` (re-runs effect) | — | as above | as above | Re-fetches metadata | active guard |
| A3 | Select preview row | `setSelectedItemIndex(i)` | — | — | — | Selects item for media-format panel | bounds |
| A4 | Select media format | `selectMediaFormat(idx)` | — (local parsedItems) | — | — | Sets chosen stream; updates filename/size | selectedFormat bounds |
| A5 | Browse Save Location | `handleBrowse` → `open({directory:true})` | — | — | — | Sets save location; marks manual | dialog result |
| A6 | Target Queue select | `setSelectedQueueId` | — | — | — | Sets target queue | queue exists |
| A7 | Connections slider | `setConnections` | — | — | — | 1–16 (disabled for media) | min/max |
| A8 | Limit speed per file (checkbox+input) | `setSpeedLimitEnabled`/`setSpeedLimit` | — | — | — | Per-file KiB/s cap | numeric |
| A9 | Use authorization (checkbox+user/pass) | `setUseAuth`/`setUsername`/`setPassword` | — | — | — | Sends basic auth | useAuth flag |
| A10 | Advanced toggle | `setAdvancedExpanded` | — | — | — | Expands advanced fields | — |
| A11 | Verify Checksum (algo+digest) | `setChecksumEnabled`/`setChecksumAlgo`/`setChecksumValue` | — | — | — | Sends `algo=digest` | checksumEnabled |
| A12 | Headers / Cookies / Mirrors | `setHeaders`/`setCookies`/`setMirrors` | — | — | — | Raw strings forwarded | trim/empty |
| A13 | Add to Queue | `handleStart(false)` → `executeAddDownloads(false,…)` | `addDownload` (status `'paused'`) | `enqueue_download` (via dispatchItem) | `enqueue_download` | Adds paused items | duplicate detection |
| A14 | Start Downloads | `handleStart(true)` → `executeAddDownloads(true,…)` | `addDownload` (status `'queued'`) + immediate dispatch | `enqueue_download` | `enqueue_download` | Adds + starts items | duplicate detection |
| A15 | Cancel | `toggleAddModal(false)` | `useDownloadStore.toggleAddModal` | — | — | Closes modal | — |

**Duplicate detection** (`handleStart` → `DuplicateResolutionModal`):
- Checks URL already in queue (`status !== failed/completed`) and file exists on
  disk via `check_file_exists` and in-store path match.
- `DuplicateResolutionModal`: per-conflict **Rename** / **Replace** (file only) /
  **Skip** select → on Confirm calls `executeAddDownloads(…,resolutions)`.

| # | Action | Component / fn | Store/IPC | Rust fn | Expected behavior | Validation |
|---|--------|----------------|-----------|---------|-------------------|------------|
| A16 | Pick resolution per conflict | `updateResolution` (local) | — | — | rename/replace/skip | reason.type gates replace |
| A17 | Continue | `onConfirm(resolutions)` → `executeAddDownloads` | store+IPC | `enqueue_download`; `delete_file` (replace) | Applies resolutions | loop bounds |

⚠️ **A14 "Start Downloads"** does **not** show any progress/confirmation that
duplicate conflicts were resolved; resolution loop uses `check_file_exists` only
to gate 'rename', but the rename loop caps at count < 1000 and silently keeps
the last tried name if exceeded.

---

## 3. Download Properties Window (`PropertiesModal.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| P1 | Edit URL | `setUrl` | — | — | — | Saves on Save | non-empty on save |
| P2 | Edit File name | `setFileName` | — | — | — | Saves on Save | non-empty on save |
| P3 | Select Save Location | `handleBrowse` → `open({directory})` | — | — | — | Sets destination | dialog result |
| P4 | Connections | `setConnections` (1–16) | — | — | — | per-file connections | disabled if transferLocked |
| P5 | Limit speed (checkbox+value) | `setSpeedLimitEnabled`/`setSpeedLimitValue` | — | — | — | per-file KiB/s | disabled if transferLocked |
| P6 | Login mode toggle (matching/custom/none) | `setLoginMode` | — | — | — | Credentials source | disabled if transferLocked |
| P7 | Custom username/password | `setUsername`/`setPassword` | — | — | — | Override login | custom mode only |
| P8 | Checksum (verify+algo+digest) | `setChecksumEnabled`/`setChecksumAlgorithm`/`setChecksumValue` | — | — | — | `algo=digest` | disabled if transferLocked |
| P9 | Cookies / Headers / Mirrors | `setCookies`/`setHeaders`/`setMirrors` | — | — | — | Advanced fields | disabled if transferLocked |
| P10 | Advanced toggle | `setAdvancedExpanded` | — | — | — | Expand/collapse | — |
| P11 | Save | `handleSave` → `applyProperties(id, updates)` | `useDownloadStore.applyProperties` | `remove_from_queue`/`detach_download_for_reconfigure`/`enqueue_download` (re-dispatch) | `remove_from_queue`, `detach_download_for_reconfigure`, `enqueue_download` | Updates item; re-attaches backend | status guard (throws if active) |
| P12 | Cancel | `setSelectedPropertiesDownloadId(null)` | as above | — | — | Closes modal | — |

`applyProperties` rules (`useDownloadStore.ts:313`):
- **downloading/processing/retrying** → throws "Cannot change properties while
  transfer is active."
- **completed/failed** → updates store only (read-only identity; settings saved
  for redownload).
- **queued** → removes from queue, updates, re-dispatches; on dispatch failure
  removes from queue.
- **paused** → `detach_download_for_reconfigure`, unregister, update (no
  re-dispatch; resumes on next resume).

⚠️ **P5/P8/P9** use `isTransferLocked` (downloading/processing/retrying) to
disable inputs, but `isLocked` also includes `completed`. For a **completed**
item the URL/filename/destination are disabled (correct), yet speed-limit,
checksum, cookies, headers, mirrors are **editable** — these only matter for
redownload, which is the documented intent, but the disable logic is split
across two booleans (`isLocked` vs `isTransferLocked`) and is easy to misread.

---

## 4. Settings Window (`SettingsView.tsx`, tabbed)

| # | Action | Tab | Store setter | IPC command | Rust fn | Validation |
|---|--------|-----|--------------|-------------|---------|------------|
| T1 | Tab select | — | `setActiveSettingsTab` | — | — | enum |
| **Downloads** | | | | | | |
| SD1 | Default connections (1–16) | downloads | `setPerServerConnections` | — | — | onBlur clamp 1–16 |
| SD2 | Parallel downloads (1–12) | downloads | `setMaxConcurrentDownloads` | `set_concurrent_limit` (App effect) | `set_concurrent_limit` | onBlur clamp 1–12 |
| SD3 | Global speed limit | downloads | `setGlobalSpeedLimit` | `set_global_speed_limit` (App effect) | `set_global_speed_limit` | free text |
| SD4 | Automatic retries (0–10) | downloads | `setMaxAutomaticRetries` | — | — | onBlur clamp 0–10 |
| SD5 | Show notification on completion | downloads | `setShowNotifications` | — | — | toggle |
| SD6 | Play sound on completion | downloads | `setPlayCompletionSound` | — | — | requires SD5 |
| **Look & Feel** | | | | | | |
| SL1 | Theme (system/light/dark/dracula/nord) | lookandfeel | `setTheme` | — | — | App effect applies classes |
| SL2 | Font size | lookandfeel | `setAppFontSize` | — | — | `data-font-size` attr |
| SL3 | List row density | lookandfeel | `setListRowDensity` | — | — | enum |
| SL4 | Dock badge | lookandfeel | `setShowDockBadge` | `update_dock_badge` (setter + effect) | `update_dock_badge` | toggle; clears badge when off |
| SL5 | Menu bar icon | lookandfeel | `setShowMenuBarIcon` | `toggle_tray_icon` (App effect) | `toggle_tray_icon` | toggle |
| **Network** | | | | | | |
| SN1 | Proxy mode (none/system/custom) | network | `setProxyMode` | — | — | radio |
| SN2 | Proxy host | network | `setProxyHost` | — | — | shown if custom |
| SN3 | Proxy port (1–65535) | network | `setProxyPort` | — | — | onBlur clamp |
| SN4 | Custom User Agent | network | `setCustomUserAgent` | — | — | free text + datalist |
| **Locations** | | | | | | |
| SO1 | Default download path (text+Browse) | locations | `setDefaultDownloadPath` | — | — | `pickDirectory` |
| SO2 | Ask where to save each file | locations | `setAskWhereToSaveEachFile` | — | — | toggle |
| SO3 | All Categories Base Browse | locations | `setCategoryDirectory` ×7 | `create_category_directories` | `parity::create_category_directories` | creates dirs on disk |
| SO4 | Per-category path (text+Browse) | locations | `setCategoryDirectory` | — | — | `handleBrowseCategory` |
| SO5 | Reset Defaults | locations | `resetCategoryDirectories` | — | — | restores defaults |
| **Site Logins** | | | | | | |
| SLg1 | Delete login | sitelogins | `removeSiteLogin` | `delete_keychain_password` | `delete_keychain_password` | removes keychain entry |
| SLg2 | Add login (pattern/user/pass) | sitelogins | `addSiteLogin` | `set_keychain_password` | `set_keychain_password` | pattern+user non-empty |
| **Power** | | | | | | |
| SP1 | Prevent sleep while downloading | power | `setPreventsSleepWhileDownloading` | `set_prevent_sleep` (syncSystemIntegrations + setter) | `set_prevent_sleep` | keepawake on/off |
| **Engine** | | | | | | |
| SE1 | Recheck engines | engine | — | `get_*_engine_status` ×4 | `check_*` | force re-run; cached otherwise |
| SE2 | Show/hide technical details | engine | — (local) | — | — | expand card |
| SE3 | Browser Cookies Source | engine | `setMediaCookieSource` | — | — | enum (none/safari/chrome/firefox/edge/brave) |
| **Integrations** | | | | | | |
| SI1 | Copy Token | integrations | — (clipboard) | — | — | copies pairing token |
| SI2 | Regenerate token | integrations | `regeneratePairingToken` | `set_keychain_password` + `set_extension_pairing_token` (App effect) | both | mints new token |
| SI3 | Get Extension links | integrations | — (anchor) | — | — | opens external URLs |
| **About** | | | | | | |
| SA1 | Check for Updates | about | — | `check_for_updates` | `parity::check_for_updates` | toast result |
| SA2 | Auto-check updates toggle | about | `setAutoCheckUpdates` | — | — | switch |

---

## 5. Tools Views

### 5.1 Scheduler (`SchedulerView.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| SC1 | Enable Scheduler | `updateDraft('enabled')` | — (draft) | — | — | Toggles draft (saved on Save) | — |
| SC2 | Start/Stop time | `updateDraft('startTime'/'stopTime'/'stopTimeEnabled')` | — | — | — | Sets schedule window | time input |
| SC3 | Run Every Day | `updateDraft('everyday')` | — | — | — | All days | — |
| SC4 | Day toggles | `toggleDay` | — | — | — | Selected days | — |
| SC5 | Post-queue action radio | `updateDraft('postQueueAction')` | — | — | — | none/sleep/restart/shutdown | — |
| SC6 | Run Now | `runNow` → `startQueue(MAIN_QUEUE_ID)` | `setSchedulerRunning(true)` | enqueue/resume | dispatchItem | Starts main queue now | count>0 |
| SC7 | Pause | `pauseNow` → `pauseQueue(MAIN_QUEUE_ID)` | `setSchedulerRunning(false)` | `pause_download` | `pause_download` | Pauses main queue | — |
| SC8 | Save Settings | `save` → `setScheduler(normalized)` | `setScheduler` | — | — | Persists scheduler; registered by scheduler.rs | selectedDays fallback |
| SC9 | Grant/Revoke Automation | `handlePermissionAction` | — | `request_automation_permission` / `open_automation_settings` | both | macOS Automation for Finder | platform isMac |

### 5.2 Speed Limiter (`SpeedLimiterView.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| VL1 | Enable toggle | `setEnabled` | — (local) | — | — | Enables draft | — |
| VL2 | Value input | `setValue` | — | — | — | numeric | min 1; clamp |
| VL3 | Unit KB/s · MB/s | `setUnit` | — | — | — | unit select | — |
| VL4 | Quick presets (1/5/10 MB/s) | `preset(n)` | — | — | — | sets value+unit | — |
| VL5 | Save Limit | `save` → `setGlobalSpeedLimit` + `setLastCustomSpeedLimitKiB` | both | `set_global_speed_limit` (App effect) | `set_global_speed_limit` | Applies global cap | clamp; disabled→'' |

### 5.3 Diagnostics (`DiagnosticsView.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| DG1 | Clear console | `handleClear` → `setLogs([])` | — (local) | — | — | Empties log view | — |
| DG2 | Export Logs | `handleExport` → `save({…})` | — | `export_logs` | `lib.rs:2800 export_logs` | Writes log to chosen path | path from save dialog |

---

## 6. Menus / Tray

### 6.1 Tray menu (built twice — see risk note)

Primary tray built in `setup()` (`lib.rs:3192`) with items: **Show Firelink,
Pause All, Resume All, Quit**. Menu events emit `tray-action` events handled by
`downloadStore.ts:79` (true global pause/resume across all queues). Left-click
tray icon restores main window.

`toggle_tray_icon` (`lib.rs:2822`) **rebuilds** the tray when "Menu bar icon" is
toggled, but only registers **Show / Quit** — it omits the Pause All / Resume All
items. Because `toggle_tray_icon` checks `tray_by_id("main")` and the setup tray
already used id `"main"`, toggling on is a no-op while the setup tray exists.

| # | Action | Trigger | Handler | IPC / Event | Rust fn | Expected behavior |
|---|--------|---------|---------|-------------|---------|-------------------|
| TR1 | Show Firelink | tray menu | `restore_main_window` | — | `restore_main_window` | Unminimize/show/focus |
| TR2 | Pause All | tray menu | emits `tray-action` 'pause-all' | event | — | Pauses all queues |
| TR3 | Resume All | tray menu | emits `tray-action` 'resume-all' | event | — | Starts all queues |
| TR4 | Quit | tray menu | `app.exit(0)` | — | — | Exits app |
| TR5 | Tray left-click | icon click | `restore_main_window` | — | — | Show window |

⚠️ There is **no native macOS app menu** (no Edit menu with Cut/Copy/Paste /
Cmd+Q, no Window menu). Standard keyboard shortcuts and clipboard editing rely
on the webview default. This is a UX gap on macOS.

### 6.2 Window controls

Close/minimize/maximize are the native Tauri decorations (title bar hidden;
`WindowDragRegion` provides drag only). Close behavior default (no
minimize-to-tray override observed).

---

## 7. Delete Confirmation (`DeleteConfirmationModal.tsx`)

| # | Action | Component / fn | Store action | IPC command | Rust fn | Expected behavior | Validation |
|---|--------|----------------|--------------|-------------|---------|-------------------|------------|
| D1 | Remove (from list only) | `handleRemoveFromList` → `removeDownload(id,false)` | `useDownloadStore.removeDownload` | `remove_download` (filepath null) | `remove_download` | Removes item; keeps file | isRemoving flag |
| D2 | Delete file | `handleDeleteFile` → `removeDownload(id,true)` | `removeDownload` | `trash_download_assets` + `remove_download` | `trash_download_assets`, `remove_download` | Trashes file+.aria2+.part, removes item | path resolve |
| D3 | Cancel | `handleCancel` → `closeDeleteModal` | `closeDeleteModal` | — | — | Closes modal | — |

---

## 8. IPC Command → Rust function map

All commands declared in `src/ipc.ts` `CommandMap`. Registered handlers:
`src-tauri/src/lib.rs:3550 generate_handler![…]`.

| IPC command | Rust fn | File:line | Status |
|-------------|---------|-----------|--------|
| `fetch_metadata` | `fetch_metadata` | lib.rs:616 | ✅ |
| `fetch_media_metadata` | `fetch_media_metadata` | lib.rs:784 | ✅ |
| `get_engine_status` | `get_engine_status` | lib.rs:1674 | ✅ |
| `get_aria2_engine_status` | `get_aria2_engine_status` | lib.rs:1694 | ✅ |
| `get_ytdlp_engine_status` | `get_ytdlp_engine_status` | lib.rs:1702 | ✅ |
| `get_ffmpeg_engine_status` | `get_ffmpeg_engine_status` | lib.rs:1707 | ✅ |
| `get_deno_engine_status` | `get_deno_engine_status` | lib.rs:1712 | ✅ |
| `open_file` | `open_file` | lib.rs:1018 | ✅ (unused by UI; UI uses open_downloaded_file) |
| `show_in_folder` | `show_in_folder` | lib.rs:1031 | ✅ (unused by UI; UI uses reveal_in_file_manager) |
| `reveal_in_file_manager` | `commands::reveal_in_file_manager` | commands.rs:6 | ✅ |
| `open_downloaded_file` | `commands::open_downloaded_file` | commands.rs:27 | ✅ |
| `trash_download_assets` | `commands::trash_download_assets` | commands.rs:45 | ✅ |
| `pause_download` | `pause_download` | lib.rs:2137 | ✅ |
| `resume_download` | `resume_download` | lib.rs:2211 | ✅ |
| `remove_download` | `remove_download` | lib.rs:2292 | ✅ |
| `detach_download_for_reconfigure` | `detach_download_for_reconfigure` | lib.rs:2364 | ✅ |
| `enqueue_download` | `enqueue_download` | lib.rs:2565 | ✅ |
| `enqueue_many` | `enqueue_many` | lib.rs:2586 | ✅ |
| `move_in_queue` | `move_in_queue` | lib.rs:2613 | ✅ |
| `remove_from_queue` | `remove_from_queue` | lib.rs:2622 | ✅ |
| `get_pending_order` | `get_pending_order` | lib.rs:2560 | ✅ |
| `set_concurrent_limit` | `set_concurrent_limit` | lib.rs:2636 | ✅ |
| `set_global_speed_limit` | `set_global_speed_limit` | lib.rs:2663 | ✅ |
| `update_dock_badge` | `update_dock_badge` | lib.rs:2507 | ✅ |
| `set_prevent_sleep` | `set_prevent_sleep` | lib.rs:2526 | ✅ |
| `perform_system_action` | `perform_system_action` | lib.rs:2555 | ✅ |
| `request_automation_permission` | `request_automation_permission` | lib.rs:2680 | ✅ |
| `open_automation_settings` | `open_automation_settings` | lib.rs:2707 | ✅ |
| `get_free_space` | `get_free_space` | lib.rs:2721 | ✅ |
| `set_keychain_password` | `set_keychain_password` | lib.rs:2758 | ✅ |
| `get_keychain_password` | `get_keychain_password` | lib.rs:2765 | ✅ |
| `delete_keychain_password` | `delete_keychain_password` | lib.rs:2771 | ✅ |
| `check_file_exists` | `check_file_exists` | lib.rs:2778 | ✅ |
| `delete_file` | `delete_file` | lib.rs:2787 | ✅ |
| `toggle_tray_icon` | `toggle_tray_icon` | lib.rs:2823 | ⚠️ see TR note |
| `set_extension_pairing_token` | `set_extension_pairing_token` | lib.rs:2875 | ✅ |
| `set_extension_frontend_ready` | `set_extension_frontend_ready` | lib.rs:2892 | ✅ |
| `get_system_proxy` | `parity::get_system_proxy` | parity.rs:7 | ✅ |
| `get_file_category` | `parity::get_file_category` | parity.rs:22 | ✅ (UI uses TS `categoryForFileName` instead) |
| `check_for_updates` | `parity::check_for_updates` | parity.rs:84 | ✅ |
| `is_supported_media` | `parity::is_supported_media` | parity.rs:180 | ✅ (declared in ipc.ts but UI uses TS `isMediaUrl`/`get_supported_media_domains`) |
| `create_category_directories` | `parity::create_category_directories` | parity.rs:144 | ✅ |
| `export_logs` | `export_logs` | lib.rs:2800 | ✅ |
| `db_save_settings` | — | — | ❌ declared in `ipc.ts` but **not registered** in Rust |
| `db_load_settings` | — | — | ❌ declared in `ipc.ts` but **not registered** in Rust |
| `db_get_all_downloads` | — | — | ❌ declared in `ipc.ts` but **not registered** in Rust |
| `db_save_download` | — | — | ❌ declared in `ipc.ts` but **not registered** in Rust |
| `db_delete_download` | — | — | ❌ declared in `ipc.ts` but **not registered** in Rust |
| `start_download` | — | — | ❌ declared in `ipc.ts` (`StartDownloadArgs`) but **not registered** in Rust |
| `start_media_download` | — | — | ❌ declared in `ipc.ts` (`StartMediaDownloadArgs`) but **not registered** in Rust |
| `get_supported_media_domains` | `parity::get_supported_media_domains` | parity.rs:175 | ✅ (called directly via `invoke` in `downloads.ts`, not in typed `CommandMap`) |

> Persistence actually runs through the Tauri `plugin-store` (`LazyStore` on
> `store.bin`) in the Zustand subscriber (`useDownloadStore.ts:646`), not the
> `db_*` commands — those are dead declarations. Likewise `start_download` /
> `start_media_download` are superseded by `enqueue_download`/`enqueue_many` via
> the `dispatchItem` flow.

### Backend events (Rust → Frontend)

| Event | Payload | Emitted by | Listened by |
|-------|---------|------------|-------------|
| `download-progress` | `DownloadProgressEvent` | download/queue tasks | `downloadStore.ts:31` |
| `download-state` | `DownloadStateEvent` | pause/resume/remove + tasks | `downloadStore.ts:49`, `App.tsx:210` |
| `download-complete` / `download-failed` | string | (declared; not emitted in read code) | — |
| `schedule-trigger` | 'start'\|'stop' | scheduler.rs | `App.tsx:113` |
| `tray-action` | 'pause-all'\|'resume-all' | tray menu | `downloadStore.ts:80` |
| `extension-add-download` | `ExtensionDownload` | extension_server | `App.tsx:231` |
| `extension-downloads-queued` | `DownloadItem[]` | extension_server | `App.tsx:234` |
| `deep-link-add-download` | string | deep-link dispatch | `App.tsx:251` |
| `log` | `{level,message}` | tauri-plugin-log | `DiagnosticsView.tsx:19` |

---

## 9. Highest-Risk Areas

Ranked by likelihood × impact:

1. **Tray menu divergence / duplicate tray builder** (`lib.rs:3192` vs
   `lib.rs:2822`). The setup tray registers Pause All / Resume All; the toggle
   builder does not and silently no-ops when the setup tray already exists.
   Toggling "Menu bar icon" off then on can leave the user with a tray that has
   no pause/resume items, or no tray at all depending on ordering. **Risk: lost
   global queue control + confusing state.**

2. **Resume/Pause All scope mismatch** (M2/M3 vs TR2/TR3). Toolbar Resume/Pause
   All only act on the **current filter**, while tray Resume/Pause All act on
   **all queues**. Same icon, different semantics — users will mis-pause and
   leave downloads running, or fail to resume paused items.

3. **`perform_system_action` (Sleep/Restart/Shutdown)** is invoked with no
   confirmation dialog from the scheduler path (`App.tsx:139`) once the queue
   drains. A user who enables "Shut down" and walks away loses unsaved work in
   other apps. Permission is checked, but no undo. **Risk: destructive,
   cross-application.**

4. **`applyProperties` state machine** (`useDownloadStore.ts:313`). Five
   branches (active throws, completed/failed store-only, queued re-dispatch,
   paused detach) with backend registration tracked in a separate `Set`. A
   failed `detach_download_for_reconfigure` throws and preserves old props, but
   a failed re-dispatch on queued silently removes from queue. Easy to reach a
   state where the UI shows "queued" but the backend has no task
   (`backendRegisteredIds` desync).

5. **`dispatchItem` in-flight dedupe + failure → 'failed'**
   (`useDownloadStore.ts:21`). On enqueue failure the item is marked `failed`
   without surfacing the error to the user (only `console.error`). Users see a
   mysteriously failed download with no diagnostic.

6. **Path authorization surface** (`commands.rs`). `open_downloaded_file`,
   `reveal_in_file_manager`, `trash_download_assets` all gate on
   `known_download_paths` (registered via `download_ownership`). If an item is
   redownloaded or its destination changes, stale registration could either
   block a legitimate open or — worse — the `delete_file` path used by
   duplicate "Replace" (`AddDownloadsModal` A17) bypasses ownership checks and
   uses `is_safe_path` only. **Risk: deleting the wrong file if a crafted path
   passes `is_safe_path`.**

7. **Dead IPC declarations** (`start_download`, `start_media_download`, all
   `db_*`). Not exploitable, but a maintenance trap: any future caller using the
   typed `invokeCommand` for these will get a runtime "command not found" with
   no compile-time signal.

8. **No native macOS app menu.** No Edit menu (cut/copy/paste/select-all),
   Cmd+Q, or Window menu. Clipboard paste into the Add-URLs textarea works only
   via the webview default; the global paste handler (G1) intentionally ignores
   inputs, so Cmd+V in the textarea is the only path — fine, but no menu affordance.

9. **Duplicate-resolution rename loop** (A17) caps at 1000 attempts and on
   exhaustion keeps the last tried name without warning, which could collide
   and overwrite via the subsequent `addDownload` if the backend doesn't guard.

10. **`SpeedLimiterView` Save applies via effect** (`App.tsx:101`), not directly.
    The `previousSpeedLimit` ref dedupes, but the indirection means the visible
    toast ("Global limit saved…") can fire before the backend actually accepts
    the limit, and a failed `set_global_speed_limit` is only logged.
