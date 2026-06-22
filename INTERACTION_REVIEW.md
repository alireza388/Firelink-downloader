# Firelink Interaction Inventory and Modernization Review

Static, inventory-only review of the current React/Tauri code. No application
click-through or manual UI testing was performed.

## Implementation update — June 21, 2026

The following recommendations from this review have now been implemented:

- all new and migrated downloads receive an explicit Main Queue identity unless
  the user chooses another queue;
- shared action-policy helpers now drive Start, Resume, Pause, Redownload, and
  Properties lock behavior;
- tray Pause/Resume All uses centralized global store actions, and tray
  reconstruction retains the complete menu;
- queued, processing, and retrying lifecycle actions are handled consistently;
- Show in Finder now sends the exact owned output path for completed and partial
  downloads;
- duplicate replacement no longer suppresses deletion errors, blocks replacement
  of active transfers, reports batch failures, and fails explicitly when no
  rename candidate is available;
- bulk removal reports partial failures instead of failing an opaque concurrent
  batch;
- List Row Density and Automatically Check for Updates are functional;
- update notifications include an action to open the release;
- the duplicate global-speed editor was replaced with a single Speed Limiter
  configuration path and its capability copy is now accurate;
- extension server status reports the actual bound port;
- queue assignment, IPC event listening, path resolution, and editable-state
  policy are centralized;
- the enqueue payload now uses a Rust-generated TypeScript binding;
- column widths persist, menus close with Escape, async clipboard/keychain/log
  export failures are surfaced, and fatal render errors provide recovery;
- the confirmed-dead legacy `QualityModal` and its unused metadata store state
  were removed. The active Add Downloads media-format flow and all backend media
  format/progress/size logic were preserved.

The following larger architectural items remain recommendations rather than
being forced into this change:

- generating the complete command/event client instead of maintaining command
  names in `src/ipc.ts`;
- moving all settings migration logic to one generated cross-language schema;
- replacing custom menus with a complete focus-managed menu primitive;
- making scheduler runs backend-owned objects with explicit run membership;
- adding request-level acknowledgement from the app UI back to automatic
  browser-download capture.

## Status and validation legend

- **wired**: the visible control reaches the intended state/IPC/backend path.
- **partially wired**: the main path exists, but a setting, branch, result, or
  failure state is not fully applied.
- **unwired/dead**: code or a visible control exists without an effective
  consumer.
- **fragile**: behavior depends on duplicated state, timing, implicit status
  rules, swallowed failures, or incomplete synchronization.
- **duplicated**: materially similar behavior exists in multiple handlers.
- **outdated**: the implementation works or persists, but no longer matches the
  current interaction architecture or expected desktop behavior.
- **unsafe**: destructive or privileged behavior lacks a sufficiently strong
  confirmation/error boundary.
- **Manual QA needed**: static inspection cannot confirm native dialog,
  notification, menu, focus, accessibility, or operating-system behavior.

Validation methods named below are non-UI methods: **code trace**, **type
check**, **unit test**, **integration test**, **IPC contract check**, and
**build check**.

## Executive risk inventory

1. **Queue identity and global controls are inconsistent.** `start-now` and
   `add-to-list` downloads have no `queueId`, while tray Pause/Resume All and
   Scheduler Run Now operate by queue IDs. Those downloads can be omitted from
   global actions. Status: **fragile / partially wired**.
2. **Tray menu rebuilding changes its action set.** Startup creates Show,
   Pause All, Resume All, Quit; re-enabling the menu-bar icon creates only Show
   and Quit. Status: **duplicated / partially wired**.
3. **Several settings advertise behavior that is not consumed.** List Row
   Density and Automatically Check for Updates are persisted but have no
   runtime consumer. Status: **unwired/dead**.
4. **Destructive duplicate replacement suppresses deletion failure.** The
   existing list item may be removed before `delete_file` fails, and the
   replacement continues. Status: **unsafe / fragile**.
5. **Action availability is implemented separately in rows, single-select
   menus, multi-select menus, toolbar controls, queue controls, tray handlers,
   and scheduler handlers.** Their status sets already differ, especially for
   `processing` and `retrying`. Status: **duplicated / fragile**.
6. **The frontend IPC wrapper is incomplete and handwritten.** Rust-generated
   data types exist, but command/event names and payloads are manually mapped;
   some live calls bypass the wrapper and some mapped commands are legacy.
   Status: **outdated / fragile**.
7. **Legacy media-quality state and component are dead.** `QualityModal`,
   `activeMetadata`, `fetchMetadataAction`, and `activeDownloadId` are not
   connected to the rendered app; the active add flow has its own media
   selection implementation. Status: **unwired/dead / duplicated**.

---

## 1. Main window

### 1.1 Window shell, toolbar, and status bar

| ID | UI action | Component / function | Store action | IPC command | Rust function | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|---|
| MW-01 | Drag window | `WindowDragRegion.onPointerDown`; title-bar drag regions | none | Tauri window API | Tauri runtime | Move the native window | wired; Manual QA needed | code trace, build check |
| MW-02 | Resize sidebar | `App.startSidebarResize` | local `sidebarWidth`, `localStorage` | none | none | Resize sidebar from 190–260 px and persist width | wired; fragile pointer-only interaction; Manual QA needed | code trace |
| MW-03 | Hide sidebar | `Sidebar` title control | `toggleSidebar` | none | none | Collapse sidebar | wired | code trace |
| MW-04 | Show sidebar | `DownloadTable` title control | `toggleSidebar` | none | none | Restore sidebar | wired | code trace |
| MW-05 | Add Download | `DownloadTable` plus button | `toggleAddModal(true)` | none | none | Open Add Downloads | wired | code trace |
| MW-06 | Resume All in current view | `DownloadTable` toolbar loop | `resumeDownload` per matching row | `resume_download`, sometimes `enqueue_download` | `resume_download`, `enqueue_download` | Start/resume ready or paused items visible in the current filter | wired but fragile: label sounds global and excludes failed/retrying | unit test, IPC contract check |
| MW-07 | Pause All in current view | `DownloadTable` toolbar loop | none; direct handler | `pause_download` per downloading row | `pause_download` | Pause downloading items visible in the current filter | wired but fragile: excludes processing/retrying and is not global | unit test, integration test |
| MW-08 | Read active/queued/done counts | `App` status bar | derived store state | none | none | Show application totals | wired; `queued` excludes `ready` | code trace |

Recommendation: replace the three toolbar buttons with a macOS-style primary
Add split button plus an overflow menu for view-scoped Start/Pause actions.
Name scope explicitly, for example “Pause Visible Downloads.” Route all status
eligibility through shared action selectors.

### 1.2 Download list selection and row actions

| ID | UI action | Component / function | Store action | IPC command | Rust function | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|---|
| MW-09 | Select row | `DownloadTable.handleItemClick` | local `selectedIds` | none | none | Select one row | wired | unit test |
| MW-10 | Toggle multi-selection | same; Cmd/Ctrl click | local `selectedIds` | none | none | Add/remove row from selection | wired; Manual QA needed for platform modifiers | unit test |
| MW-11 | Range selection | same; Shift click | local `selectedIds`, `lastSelectedId` | none | none | Select visible range | wired; fragile when filtering/reordering changes anchor | unit test |
| MW-12 | Right-click row | `DownloadItem.onContextMenu` | local context-menu state | none | none | Select row if needed and open menu | wired; Manual QA needed | code trace |
| MW-13 | Options button | `DownloadItem` overflow button | local context-menu state | none | none | Open same menu | wired | code trace |
| MW-14 | Double-click completed row | `handleDownloadDoubleClick` → `openDownloadFile` | none | `open_downloaded_file` | `commands::open_downloaded_file` | Open owned downloaded file | wired; Manual QA needed | IPC contract check, integration test |
| MW-15 | Double-click unfinished row | `handleDownloadDoubleClick` → `openProperties` | `setSelectedPropertiesDownloadId` | none | none | Open Properties | wired | code trace |
| MW-16 | Move queued item up/down | `DownloadItem` hover controls | `moveInQueue` | `move_in_queue` | `QueueManager::move_in_queue` | Reorder backend pending queue | wired; visible order remains download-list order rather than pending order | integration test |
| MW-17 | Pause row | `DownloadItem` hover control | none | `pause_download` | `pause_download` | Pause downloading, processing, or retrying item | wired; errors only reach console | integration test |
| MW-18 | Start/Resume row | `DownloadItem` hover control | `resumeDownload` | `resume_download`, fallback `enqueue_download` | `resume_download`, `enqueue_download` | Start ready item or resume paused item | wired; errors only reach console | unit test, integration test |
| MW-19 | Resize table column | `DownloadTable.startColumnResize` | local `columnWidths` | none | none | Resize column until view remount | wired; outdated because widths are not persisted and pointer-only; Manual QA needed | code trace |

Recommendation: introduce a `DownloadAction` enum and a single
`getAvailableActions(download, selectionContext)` function. Row buttons,
context menus, toolbar actions, keyboard commands, tray actions, and queue
actions should call the same handlers and eligibility rules.

### 1.3 Single-selection context menu

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| CM-01 | Open | `openDownloadFile` | none | `open_downloaded_file` / `commands::open_downloaded_file` | Open completed owned file | wired; Manual QA needed | integration test |
| CM-02 | Show in Finder | `revealDownloadFile` | none | `reveal_in_file_manager` / `commands::reveal_in_file_manager` | Reveal completed file, partial file, or known destination | partially wired: backend requires an owned file path, but unfinished UI may send only a directory | integration test |
| CM-03 | Pause | `handlePause` | none | `pause_download` / `pause_download` | Pause queued/downloading/retrying item | wired, but menu omits `processing` although row action includes it | unit test |
| CM-04 | Start/Resume | `handleResume` | `resumeDownload` | `resume_download`, `enqueue_download` | Start ready or resume paused/failed/retrying | fragile: retrying exposes both Pause and Resume in the same menu | unit test, integration test |
| CM-05 | Redownload | `redownload` | `redownload` | `enqueue_download` / `enqueue_download` | Create and immediately enqueue a copy | wired; status is initialized as queued and queue identity may be absent | unit test |
| CM-06 | Add to Queue | inline menu handler | `updateDownload({queueId})` | none | Reassign logical queue | partially wired: frontend grouping changes but no explicit backend queue/order reconciliation | integration test |
| CM-07 | Copy Address | clipboard handler | none | browser clipboard | Copy URL | wired; Manual QA needed | code trace |
| CM-08 | Copy File Path | clipboard handler | none | browser clipboard | Copy resolved completed path | wired; Manual QA needed | unit test |
| CM-09 | Remove | `handleDelete` | `openDeleteModal` | none | Open removal confirmation | wired | code trace |
| CM-10 | Properties | `openProperties` | `setSelectedPropertiesDownloadId` | none | Open Properties | wired | code trace |

### 1.4 Multi-selection context menu

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| CM-11 | Start/Resume selected | inline loop | `resumeDownload` | resume/enqueue commands | Start eligible selected items | wired but duplicated; eligibility differs from toolbar and single menu | unit test |
| CM-12 | Add selected to Queue | inline loop | `updateDownload({queueId})` | none | Reassign non-completed items | partially wired / duplicated | integration test |
| CM-13 | Copy selected addresses | inline clipboard handler | none | browser clipboard | Copy URLs separated by newlines | wired; no success feedback | code trace |
| CM-14 | Remove selected | `openDeleteModal(ids)` | modal state | none | Confirm bulk removal/deletion | wired; destructive operations execute concurrently | integration test |

Recommendation: use an accessible menu primitive with roving focus, Escape,
arrow-key navigation, viewport collision handling, and one action registry.
The current hover-only nested queue submenu is fragile for keyboard and trackpad
users. Manual QA is needed to confirm current focus behavior.

### 1.5 Sidebar navigation and queues

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| SB-01 | Select Library filter | `Sidebar.NavItem` | `App.setFilter`, `setActiveView('downloads')` | none | Filter All/Active/Completed/Unfinished | wired | unit test |
| SB-02 | Select category folder | `Sidebar.NavItem` | same | none | Filter by category | wired | unit test |
| SB-03 | Select queue | `QueueItem` | same | none | Filter by queue ID | wired | unit test |
| SB-04 | Add queue | add input, Enter/blur | `addQueue` | DB persistence subscription | DB replace functions | Create a persisted logical queue | wired; duplicate/blank-after-trim names are not explicitly reported | unit test |
| SB-05 | Cancel add queue | Escape | local state | none | Close input without adding | wired; blur after Escape can still run submit with current value depending event order; Manual QA needed | unit test |
| SB-06 | Rename queue | context action, input Enter/blur | `renameQueue` | DB persistence subscription | DB replace functions | Rename queue | wired; same blur/keyboard fragility | unit test |
| SB-07 | Start Queue | queue context menu | `startQueue` | resume/enqueue commands | Start queued/paused/failed items with this queue ID | wired but fragile around `hasBeenDispatched` and absent queue IDs | unit test, integration test |
| SB-08 | Pause Queue | queue context menu | `pauseQueue` | `pause_download` | Pause active items in queue | partially wired: only `downloading`, not `processing` or `retrying` | unit test |
| SB-09 | Delete Queue | queue context menu | `removeQueue` | DB persistence subscription | Reassign items to Main Queue and delete custom queue | wired; no confirmation | unit test |
| SB-10 | Open Scheduler | `ToolItem` | `setActiveView` | none | Show Scheduler | wired | code trace |
| SB-11 | Open Speed Limiter | `ToolItem` | `setActiveView` | none | Show Speed Limiter | wired | code trace |
| SB-12 | Open Logs | `ToolItem` | `setActiveView` | none | Show Logs | wired | code trace |
| SB-13 | Open Settings | footer button | `setActiveView` | none | Show Settings | wired | code trace |

Recommendation: make queue membership explicit and total. Every non-completed
download should either have a queue ID or an explicit `unassigned` state that
global commands intentionally include/exclude. Do not use optional `queueId`
as an implicit behavior switch.

---

## 2. Add Downloads window

### 2.1 Input, metadata, preview, and destination

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| AD-01 | Enter/paste URL lines | URL textarea and metadata effect | local state | `fetch_metadata` or `fetch_media_metadata` | Parse each line and show metadata | wired but fragile: any non-empty line is counted as “valid”; metadata requests are sequential | unit test, integration test |
| AD-02 | Refresh Metadata | refresh button | nonce increment | same metadata commands | Re-run metadata lookup | wired | unit test |
| AD-03 | Select preview item | preview row click/Enter/Space | local selected index | none | Select item for media format details | wired | unit test |
| AD-04 | Select media stream | `selectMediaFormat` | local parsed item | none | Change format, extension, and estimated size | wired; duplicated with dead `QualityModal` implementation | unit test |
| AD-05 | Browse save location | `handleBrowse` | local manual destination | dialog plugin | Select one shared folder | wired; Manual QA needed | code trace |
| AD-06 | Ask where to save on action | `handleAction` | reads setting | dialog plugin | Prompt before committing additions | wired; name says “each file” but one folder is selected for the whole batch | unit test, Manual QA needed |
| AD-07 | Read free space | save-location effect | local `freeSpace` | `get_free_space` / `get_free_space` | Show available space for destination | wired; path is not constrained because this is read-only | IPC contract check |

Recommendation: separate URL parsing from metadata loading. Build a validated
draft list first, reject unsupported schemes immediately, then run metadata
lookups concurrently with bounded concurrency and per-item retry/error actions.

### 2.2 Transfer and authentication controls

| ID | Action | Component / function | Store effect | Backend payload | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| AD-08 | Connections slider | local state | passed to `addDownload` | `connections` | Set 1–16 connections for non-media | wired; defaults to 16 instead of current setting; disabled if any batch item is media | unit test |
| AD-09 | Per-file speed toggle/value | local state | passed to `addDownload` | `speed_limit` | Apply per-download limit | wired; accepts invalid/zero text until backend normalization/use | unit test |
| AD-10 | Authorization toggle | local state | passed to `addDownload` | username/password | Use ad-hoc credentials | wired; session-only secrets are intentionally not persisted | unit test |
| AD-11 | Username/password | local state | passed to store | username/password | Override matching site login | wired | integration test |
| AD-12 | Advanced disclosure | local state | none | none | Show advanced fields | wired | code trace |
| AD-13 | Checksum toggle/algorithm/digest | local state | passed to store | checksum | Verify checksum | wired; digest format is not validated in UI | unit test |
| AD-14 | Headers | local state | passed to store | headers | Add request headers, including extension referer | wired; raw multiline input | integration test |
| AD-15 | Cookies | local state | passed to store | cookies | Add Cookie header | wired; raw secret input | integration test |
| AD-16 | Mirrors | local state | passed to store | mirrors | Add alternate URIs | wired; scheme/value validation deferred | integration test |

### 2.3 Commit actions and duplicate modal

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| AD-17 | Start Downloads | `handleAction({type:'start-now'})` | `addDownload` → `dispatchItem` | `enqueue_download` | Add and immediately dispatch each item | wired; per-item failures are logged and modal still closes | integration test |
| AD-18 | Add to List | action menu | `addDownload({type:'add-to-list'})` | DB persistence only | Add as ready without dispatch | wired; omitted from queue-based global actions | unit test |
| AD-19 | Add to Queue | nested action menu | `addDownload({type:'add-to-queue'})` | no immediate backend enqueue | Add as queued for selected logical queue | wired; local pending order is updated before backend registration | unit test |
| AD-20 | Cancel | footer button | `toggleAddModal(false)` | none | Close and clear pending extension/deep-link fields | wired | code trace |
| AD-21 | Choose Rename/Replace/Skip | `DuplicateResolutionModal` | local conflict state | none | Select resolution per conflict | wired; URL duplicates default to Rename although renaming does not resolve URL duplication | unit test |
| AD-22 | Continue duplicate resolution | `executeAddDownloads` | remove/add actions | `check_file_exists`, `delete_file`, enqueue commands | Apply selected resolutions and add items | unsafe / fragile: replacement delete failures are swallowed; rename loop silently stops at 999 | integration test |
| AD-23 | Cancel duplicate resolution | duplicate modal Cancel | local state | none | Return to Add Downloads without committing | wired | code trace |

Recommendation: move duplicate resolution into a transactional service that
returns typed outcomes. Never remove the existing list item until filesystem
replacement succeeds or a backend-owned overwrite operation has accepted the
request. Show a batch result summary when some additions fail.

---

## 3. Download Properties window

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|
| PR-01 | Edit URL | local state | applied by `applyProperties` | may re-enqueue later | Change source for eligible item | wired | unit test |
| PR-02 | Edit filename | local state | `applyProperties` | may re-enqueue later | Change output name | wired | unit test |
| PR-03 | Select save location | `handleBrowse` | local state | dialog plugin | Change destination | wired; Manual QA needed | code trace |
| PR-04 | Edit connections | local state | `applyProperties` | enqueue payload | Change future/reconfigured transfer | wired; no on-blur clamp | unit test |
| PR-05 | Edit speed limit | local state | `applyProperties` | enqueue payload | Change future/reconfigured transfer | wired | unit test |
| PR-06 | Matching/custom/no-login mode | local state | `applyProperties` | dispatch credential resolution | Choose credential source | wired | unit test |
| PR-07 | Custom credentials | local state | `applyProperties` | enqueue payload | Use ad-hoc credentials | wired; secret is not persisted | integration test |
| PR-08 | Advanced transfer fields | local state | `applyProperties` | enqueue payload | Change checksum/cookies/headers/mirrors | wired | unit test |
| PR-09 | Save | `handleSave` | `applyProperties` | `remove_from_queue`, `detach_download_for_reconfigure`, `enqueue_download` depending status | Apply safe changes and close | wired but fragile: behavior is status-dependent and rollback is incomplete after re-dispatch failure | unit test, integration test |
| PR-10 | Cancel | footer button | clear selected ID | none | Close without applying | wired | code trace |

Status rules:

- active `downloading`/`processing`/`retrying`: Save disabled and store rejects.
- `ready`/`completed`/`failed`: frontend store update only.
- backend-registered `queued`: remove, update, re-dispatch.
- backend-registered `paused`: detach with acknowledgement, update, re-dispatch on resume.

Recommendation: model Properties as a typed edit session with explicit modes:
`identity-editable`, `transfer-options-only`, `requires-detach`, and
`read-only`. A single policy function should drive disabled fields, copy, save,
and backend transition behavior. The current `isLocked` and
`isTransferLocked` booleans are easy to diverge.

---

## 4. Removal confirmation modal

| ID | Action | Component / function | Store action | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|---|---|
| RM-01 | Cancel | `handleCancel` | `closeDeleteModal` | none | Close without changes | wired | code trace |
| RM-02 | Remove from list | `handleRemoveFromList` | `removeDownload(id,false)` | `remove_download` | Stop backend work, remove record, retain file | wired; bulk operations run concurrently and partially completed batches remain if one fails | integration test |
| RM-03 | Delete file | `handleDeleteFile` | `removeDownload(id,true)` | `trash_download_assets`, then `remove_download` | Trash owned primary/partial assets and remove record | wired with strong ownership checks; bulk partial-failure UX is fragile | integration test |

Recommendation: use a batch command returning per-item results, then display a
clear partial-success summary. Keep the exact owned-path authorization model in
`commands.rs`; do not replace it with broad folder-prefix authorization.

---

## 5. Settings window

### 5.1 Navigation and Downloads

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-01 | Select one of nine settings tabs | `setActiveSettingsTab` | Switch pane and persist selected tab | wired | unit test |
| ST-02 | Default connections | `setPerServerConnections` | Set default 1–16 | wired; only clamped on blur | unit test |
| ST-03 | Parallel downloads | `setMaxConcurrentDownloads` → App effect `set_concurrent_limit` | Resize backend concurrency | wired; transient invalid values can reach backend before blur | integration test |
| ST-04 | Global speed limit text | `setGlobalSpeedLimit` → App effect `set_global_speed_limit` | Apply global limit | wired but duplicated with Speed Limiter; invalid input silently becomes unlimited in backend normalization | unit test |
| ST-05 | Automatic retries | `setMaxAutomaticRetries` | Set future enqueue retry count | wired; current active tasks are unchanged | unit test |
| ST-06 | Completion notifications | `setShowNotifications` | Gate terminal OS notifications | wired; permission is requested at startup regardless of setting | integration test, Manual QA needed |
| ST-07 | Completion sound | `setPlayCompletionSound` | Add sound to completion notification | wired; failed notifications never use sound | code trace, Manual QA needed |

### 5.2 Look and Feel

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-08 | Theme | `setTheme`; App root-class effect | Apply system/light/dark/Dracula/Nord | wired; Manual QA needed | code trace |
| ST-09 | Font size | `setAppFontSize`; App data attribute | Apply small/standard/large | wired; Manual QA needed | code trace |
| ST-10 | List Row Density | `setListRowDensity` | Change download-row density | **unwired/dead**: no consumer outside settings persistence | code trace, unit test |
| ST-11 | Dock badge | `setShowDockBadge`; `update_dock_badge` | Show active count | wired but duplicated between setter, App effect, and download-store sync | integration test, Manual QA needed |
| ST-12 | Menu bar icon | `setShowMenuBarIcon`; `toggle_tray_icon` | Show/hide tray | partially wired: re-created tray loses Pause/Resume All | integration test, Manual QA needed |

### 5.3 Network

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-13 | Proxy mode | `setProxyMode`; dispatch-time `getProxyArgs` | None/system/custom proxy for new dispatches | wired | unit test |
| ST-14 | Proxy host/port | setters | Build custom HTTP proxy URL | wired; host/scheme/auth validation is minimal | unit test |
| ST-15 | Custom User Agent | `setCustomUserAgent` | Apply to metadata and downloads | wired; preset strings are dated static examples | integration test |

### 5.4 Locations

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-16 | Edit base folder text | `setBaseDownloadFolder` | Change automatic category base | wired; does not create folders or verify access until used | unit test |
| ST-17 | Browse base folder | `handleBrowseBase`; `create_category_directories` | Select base and create normalized category folders | wired; directory creation warnings do not reach UI | integration test, Manual QA needed |
| ST-18 | Ask where to save each file | `setAskWhereToSaveEachFile` | Prompt during Add action | partially wired: one prompt per batch, not per file | unit test |
| ST-19 | Edit category path/subfolder | `CategoryFolderInput` | Use relative automatic subfolder or absolute override | wired but fragile: writes persisted settings on each keystroke and infers mode from string prefix | unit test |
| ST-20 | Custom folder | `handleBrowseCategory` | Set absolute category override | wired; Manual QA needed | code trace |
| ST-21 | Use automatic | clear override | Return to base/subfolder resolution | wired | unit test |
| ST-22 | Reset Defaults | `resetCategoryLocations` | Reset subfolders and overrides | wired; no confirmation | unit test |

The project already has shared frontend location normalization and a backend
settings decoder/migration layer, but equivalent migration and normalization
logic still exists in both TypeScript and Rust. Recommendation: define one
versioned persisted-settings schema and generate both bindings and migrations,
or make Rust the authoritative migration service and return normalized settings
to the frontend.

### 5.5 Site Logins

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-23 | Add login | `handleAddLogin`; `set_keychain_password`; `addSiteLogin` | Store username/pattern and password in keychain | wired; pattern validation is deferred to matching logic | integration test |
| ST-24 | Delete login | inline delete; `delete_keychain_password`; `removeSiteLogin` | Remove keychain secret and persisted metadata | fragile: keychain deletion failure is logged but metadata is still removed | integration test |

### 5.6 Power, Engines, Integrations, About

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| ST-25 | Prevent system sleep | setter plus download-state sync; `set_prevent_sleep` | Keep system awake during active downloads | wired but duplicated in setter/store synchronization | integration test, Manual QA needed |
| ST-26 | Recheck engines | four engine status commands | Validate packaged sidecars | wired | integration test, build check |
| ST-27 | Show/hide engine details | local expanded state | Reveal engine status | wired | code trace |
| ST-28 | Browser Cookies Source | `setMediaCookieSource` | Pass selected browser to media metadata/downloads | wired; Manual QA needed for browser permissions | integration test |
| ST-29 | Copy pairing token | clipboard handler | Copy keychain-hydrated token | wired; success toast is shown without awaiting clipboard result | code trace, Manual QA needed |
| ST-30 | Regenerate pairing token | `regeneratePairingToken`; keychain + App effect | Rotate token and reconfigure local server | wired but fragile: UI reports success before keychain/server calls confirm | integration test |
| ST-31 | Open extension links | external anchors | Open Firefox store or GitHub releases | wired; Manual QA needed | code trace |
| ST-32 | Check Now | `check_for_updates` | Compare stable GitHub release version and show toast | wired; available update toast has no action to open release/download | integration test |
| ST-33 | Automatically check for updates | `setAutoCheckUpdates` | Check on startup/periodically | **unwired/dead**: persisted only | code trace |
| ST-34 | Open source/license links | external anchors | Open project pages | wired; Manual QA needed | code trace |

The Integrations status text is hard-coded to “Active” and the full port range;
it is not backed by server state. Status: **outdated / partially wired**.

---

## 6. Scheduler

| ID | Action | Component / function | Store / IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|
| SC-01 | Enable Scheduler | draft checkbox | local draft then `setScheduler` on Save | Enable timed triggers | wired | unit test |
| SC-02 | Set start/stop time | draft inputs | persisted scheduler; `scheduler::spawn_scheduler` | Trigger start/stop at local time | wired; one-second polling and string-key persistence are fragile | unit test, integration test |
| SC-03 | Run Every Day / day buttons | draft controls | persisted selected days | Select schedule days | wired | unit test |
| SC-04 | Select post-queue action | radio cards | persisted enum; `perform_system_action` | Do nothing/sleep/restart/shutdown after scheduled work | wired but high-impact; no final confirmation at execution | integration test, Manual QA needed |
| SC-05 | Run Now | `runNow` | `startQueue(MAIN_QUEUE_ID)` | Start Main Queue | partially wired: misses unassigned ready/paused/failed downloads despite UI text saying all paused/failed | unit test |
| SC-06 | Pause | `pauseNow` | `pauseQueue(MAIN_QUEUE_ID)` | Pause Main Queue | partially wired: only downloading items with Main Queue ID | unit test |
| SC-07 | Save Settings | `save` | `setScheduler` | Persist normalized draft | wired | unit test |
| SC-08 | Grant permission | `handlePermissionAction` | `request_automation_permission` | Prompt/check macOS Automation | wired; command is also used as a status probe and can prompt during passive view entry; Manual QA needed | integration test |
| SC-09 | “Revoke permission” | same handler | `open_automation_settings` | Explain and open System Settings | partially wired/outdated label: app cannot revoke directly | code trace, Manual QA needed |

Recommendation: represent scheduler execution as a backend-owned state machine
with a specific run ID and set of download IDs. The frontend should observe
that run, not infer completion from all globally active statuses.

---

## 7. Speed Limiter

| ID | Action | Store / IPC | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| SP-01 | Enable/disable draft | local state | Prepare limit change | wired | unit test |
| SP-02 | Enter value | local state | Set numeric limit | wired | unit test |
| SP-03 | Select KB/s or MB/s | local state | Change unit | wired | unit test |
| SP-04 | Select 1/5/10 MB/s preset | local state | Fill common limit | wired | unit test |
| SP-05 | Save Limit | settings setters → App effect `set_global_speed_limit` | Apply backend global cap | wired; duplicated with Settings text field; UI claims active jobs are gracefully restarted, but backend only changes aria2 global option | IPC contract check, integration test |

Recommendation: keep one global speed-limit editor and one parser/formatter.
Expose backend capability/result so copy does not promise behavior for media or
native transfers that the command does not implement.

---

## 8. Logs

| ID | Action | Component / function | IPC / Rust | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|
| DG-01 | Filter severity | local `levelFilter` | none | Filter visible attached logs | wired | unit test |
| DG-02 | Clear console | `handleClear` | none | Clear only current in-memory view | wired; label could clarify that log file remains | unit test |
| DG-03 | Export Logs | `handleExport` | `export_logs` / `export_logs` | Copy current log file to selected path | wired; no success/error toast; Manual QA needed | integration test |

---

## 9. Toasts, notifications, and error surfaces

| ID | User-facing action/event | Component / function | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| NT-01 | Dismiss toast | `ToastItem` close button | Remove toast with exit animation | wired | unit test |
| NT-02 | Hover/focus toast | `ToastItem` timer logic | Pause auto-dismiss | wired | unit test |
| NT-03 | Pairing-token migration: Copy token | actionable toast in `App` | Copy token and acknowledge notice | partially wired: clipboard failure is ignored; toast is not explicitly dismissed | unit test |
| NT-04 | Pairing-token migration: Integrations | actionable toast in `App` | Open Integrations and acknowledge | wired | unit test |
| NT-05 | Download complete/failed OS notification | `App` terminal-state listener | Show native notification when enabled | wired; permission request result is ignored; Manual QA needed | integration test |
| NT-06 | Root render failure | `ErrorBoundary` | Show fatal error and component stack | wired but outdated/unsafe for production: exposes technical stack and offers no recovery action | build check |

The root toast provider is already the correct architectural direction.
Recommendation: standardize async user actions around a shared result-to-toast
helper, and ensure success is shown only after clipboard, keychain, filesystem,
or IPC completion.

---

## 10. Tray and native menus

No custom main-window application menu actions were found. The native
user-facing menu is the menu-bar/tray menu.

| ID | Action | Rust location | Frontend/store path | Expected behavior | Status | Validation |
|---|---|---|---|---|---|---|
| TR-01 | Left-click tray icon | startup and rebuilt tray handlers | `restore_main_window` | Show/focus main window | wired; Manual QA needed | integration test |
| TR-02 | Show Firelink | tray menu | `restore_main_window` | Show/focus main window | wired | integration test |
| TR-03 | Pause All | startup tray emits `tray-action` | `initDownloadListener` → `pauseQueue` per distinct queue ID | Pause all downloads | partially wired: excludes unassigned items and queue pause ignores processing/retrying | integration test |
| TR-04 | Resume All | startup tray emits `tray-action` | `startQueue` per distinct queue ID | Resume all downloads | partially wired: excludes unassigned items | integration test |
| TR-05 | Quit | tray handler | `app.exit(0)` | Exit application | wired; Manual QA needed | integration test |
| TR-06 | Close main window | `on_window_event` | hide instead of exit | Keep app running in background | wired; discoverability depends on Dock/tray state; Manual QA needed | integration test |

Recommendation: define one tray menu builder and update visibility in place.
Use backend commands for true global pause/resume rather than reconstructing
global behavior from optional frontend queue IDs.

---

## 11. Extension-triggered UI actions

### 11.1 Firefox extension popup and browser menus

| ID | Extension action | JS function/listener | App effect | Status | Validation |
|---|---|---|---|---|---|
| EX-01 | Toggle Capture All Downloads | popup `globalToggle` | Browser download interception on/off | wired | extension integration test |
| EX-02 | Disable capture on current site | popup `siteToggle` | Host-specific interception exclusion | wired; naming stores inverse boolean | unit test |
| EX-03 | Save pairing token | popup `saveTokenBtn` | Store token and probe `/ping` | wired; token remains extension-local plaintext storage by browser design | integration test |
| EX-04 | Expand/collapse token field | popup `pairingToggleBtn` | Show/hide pairing controls | wired | code trace |
| EX-05 | Toggle popup theme | popup `themeToggleBtn` | Persist popup light/dark theme | wired | code trace |
| EX-06 | Download link with Firelink | browser context menu | authenticated `/download` → app Add Downloads | wired | integration test |
| EX-07 | Download selected with Firelink | context menu + injected `content.js` | Extract selected links and open Add Downloads | wired; fallback paths are complex and duplicated | integration test |
| EX-08 | Automatic browser download capture | `chrome.downloads.onCreated` | Forward, then cancel/erase browser download after accepted response | wired but high-risk: acceptance means UI event emission, not confirmed app enqueue | integration test |
| EX-09 | Deep-link fallback | `sendToFirelink.triggerDeepLink` | `firelink://add` → `deep-link-add-download` | Open Add Downloads when local server unavailable | wired; browser fallback behavior needs Manual QA | integration test |

### 11.2 Native extension server to UI

| ID | Trigger | Backend / frontend path | Expected behavior | Status | Validation |
|---|---|---|---|---|---|
| EX-10 | Authenticated `/download` | `extension_server::download_handler` → `extension-add-download` → `handleExtensionDownload` | Wake/focus app and merge sanitized URLs into Add Downloads | wired | integration test |
| EX-11 | Deep link | deep-link parser/event → `openAddModalWithUrls` | Open Add Downloads with URLs | wired | integration test |
| EX-12 | Extension connection status shown in Settings | static Settings text | Report live server state | **unwired/dead**: text is hard-coded | code trace |

Recommendation: return a request ID from `/download`, acknowledge only after
the frontend has created a draft, and optionally return a second “queued”
result. That would let automatic browser capture cancel the browser download
only after Firelink has actually accepted responsibility.

---

## 12. IPC-backed background actions affecting UI

| ID | Background action | Frontend path | IPC / Rust | User-facing result | Status | Validation |
|---|---|---|---|---|---|---|
| BG-01 | Database hydration | `initDB` | DB load commands | Restore downloads/queues and auto-enqueue queued items | fragile: `enqueue_many` per-item results are ignored and frontend backend-registration state is not rebuilt directly | integration test |
| BG-02 | Progress events | `initDownloadListener` | `download-progress` | Update progress, speed, ETA, size | wired | integration test |
| BG-03 | State events | `initDownloadListener` | `download-state` | Update lifecycle, pending order, registration state | wired but event errors/retry reasons are not surfaced in row UI | integration test |
| BG-04 | Scheduler trigger | `App` listener | `schedule-trigger` | Start/pause Main Queue | wired with queue-scope gaps | integration test |
| BG-05 | Pairing token hydration | App startup | keychain hydration commands | Configure extension authentication and migration toast | wired | integration test |
| BG-06 | Theme system change | App media-query listener | none | Follow macOS appearance | wired; Manual QA needed | code trace |
| BG-07 | Paste outside inputs | App paste listener | none | Extract supported URLs and open Add Downloads | wired | unit test |

---

## 13. Dead, duplicated, outdated, and drift-prone implementation inventory

| Area | Evidence | Status | Practical replacement |
|---|---|---|---|
| Legacy media quality flow | `QualityModal.tsx`, `activeMetadata`, `fetchMetadataAction`, `activeDownloadId` have no rendered caller | unwired/dead, duplicated | Remove after confirming no external import; keep the Add Downloads media-format flow |
| IPC command typing | `src/ipc.ts` manually lists commands; `utils/downloads.ts` and tray listener bypass it; several listed commands are legacy UI-unused | outdated, fragile | Generate command and event bindings from Rust, including argument/result types |
| Action rules | Status checks repeated across row, toolbar, menus, queues, tray, scheduler | duplicated, fragile | Shared action enum, selectors, and command handlers |
| Tray construction | Startup and `toggle_tray_icon` build different menus | duplicated, partially wired | One builder/update function |
| Global speed editor | Settings and Speed Limiter use different input models/copy | duplicated | One shared control and parser |
| Settings decoding/migration | TypeScript normalization plus Rust decoder/migration | duplicated, fragile | One authoritative versioned schema/migration boundary |
| Category classification | TypeScript and Rust maintain separate extension lists | duplicated, drift-prone | Generate/shared data table or backend-owned classification |
| Supported media domains | Rust list copied into frontend fallback, then asynchronously refreshed | duplicated, drift-prone | Generated/static shared binding or required startup capability payload |
| Path resolution | Shared frontend resolver exists, but store also has a separate tilde resolver and backend has additional resolution/authorization paths | duplicated, fragile | One frontend resolver plus backend canonical ownership API; avoid new scattered joins |
| Boolean lock policy | `isLocked`, `isTransferLocked`, many status condition arrays | fragile | Explicit edit/action policy enums |
| Update preference | `autoCheckUpdates` persisted without consumer | unwired/dead | Startup update-check service with throttling and actionable result |
| Row density preference | `listRowDensity` persisted without consumer | unwired/dead | Root data attribute/CSS variable or remove setting |
| Integration status | Static “Active” server label | outdated | Query actual bound port/readiness and render truthful state |
| Error handling | Many clipboard, pause, folder creation, export, metadata, and deletion errors go only to console or are swallowed | fragile | Standard async action result and root toast reporting |
| Custom menus | Hand-built floating div/button menus and hover submenus | outdated/fragile | Accessible menu/split-button primitive with keyboard and collision support |
| Scheduler lifecycle | Frontend boolean plus global download scan | fragile | Backend-owned run state machine with run membership |

---

## 14. Recommended modernization sequence

### Priority 1 — behavior correctness

1. Define total queue ownership and make true global Pause/Resume backend
   commands.
2. Centralize action eligibility and execution for row/menu/toolbar/queue/tray.
3. Make duplicate replacement transactional and surface batch failures.
4. Fix tray reconstruction so the same menu is retained after hide/show.
5. Wire or remove Row Density and Automatic Update Check.

### Priority 2 — contract and lifecycle reliability

1. Generate IPC command/event bindings from Rust and prohibit raw command-name
   strings outside the generated client.
2. Reconcile `enqueue_many` results into frontend registration/status state.
3. Replace scheduler completion inference with an explicit backend run model.
4. Consolidate settings migration and location normalization around one
   authoritative schema.

### Priority 3 — modern desktop interaction

1. Use an accessible Add split button and accessible context/overflow menus.
2. Add keyboard commands for Add, Start/Resume, Pause, Remove, Properties, and
   search/filter.
3. Persist column widths and support reset-to-default.
4. Replace hard-coded extension status with live connection state.
5. Add actionable update notifications with a safe external release link.

---

## 15. Static verification matrix

Recommended non-UI checks for this inventory:

- `npm run build` — TypeScript and production frontend build.
- `npm run test -- --run` — store and location unit tests.
- `cargo check --manifest-path src-tauri/Cargo.toml` — Rust command/type check.
- `cargo test --manifest-path src-tauri/Cargo.toml --all-targets` — queue,
  ownership, settings, extension, and engine tests.
- `npm run bindings` followed by a clean diff check — generated Rust data
  binding drift.
- Add a command-registration/TypeScript-command-map contract test until the IPC
  client is generated.

Manual QA remains needed for native dialogs, Finder/open behavior, tray
visibility and focus, notification permission/sound, macOS Automation prompts,
external links, browser extension fallback behavior, menu keyboard navigation,
and pointer resizing.
