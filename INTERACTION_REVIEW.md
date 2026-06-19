# Firelink interaction/functionality review

Date: 2026-06-19

## Review method

Code-traced visible UI actions through React components, Zustand stores, typed IPC wrappers, registered Tauri commands, Rust side effects, and frontend update/error paths. Build and Rust tests were run after fixes.

## Root causes found

1. The handwritten frontend IPC map had drifted from generated Rust bindings.
   - Queue reordering sent `Up`/`Down`, but Rust deserializes `QueueDirection` as lowercase `up`/`down`.

2. Some UI buttons were visually present but not wired.
   - Add Download `Refresh Metadata` had no click handler.

3. Some UI promises did not match the backend side effect.
   - Settings global speed limit was labeled KiB/s, but bare numeric input was passed as bytes/s.
   - Download Properties allowed editing speed for active transfers even though no backend command applied a per-download active speed change.

4. Duplicate-resolution semantics were too broad.
   - The duplicate dialog offered `Replace` for URL duplicates, but replacement only makes sense for destination-file conflicts.

5. File-path actions used inconsistent destination resolution.
   - `Copy File Path` bypassed the same effective destination logic used by Open/Reveal.

6. Clipboard actions lacked visible error handling.
   - Copy failures were previously silent.

## Fixed

### Main window

| Surface/action | Status after review | Notes |
| --- | --- | --- |
| Start/resume paused or failed download | Works | Store enqueues through `enqueue_download`; existing aria2 GID resume path is handled by backend. |
| Pause active/queued/retrying download | Works | Frontend calls `pause_download`; backend removes pending task and pauses/removes active work as appropriate. |
| Remove download | Works | Frontend calls `remove_download`, then removes local persisted item. |
| Delete file with remove | Works | Uses `trash_download_assets` with backend ownership/path authorization. |
| Redownload | Works | Creates a new queued item with preserved metadata and enqueues through queue manager. |
| Open downloaded file | Works | Uses owned-path `open_downloaded_file`. |
| Show in Finder | Works for completed files/partials | Uses owned-path `reveal_in_file_manager`; non-completed double-click/menu behavior opens properties. |
| Copy URL/address | Fixed | Now reports clipboard errors. |
| Copy file path | Fixed | Now uses the same effective path resolution as file actions and reports clipboard errors. |
| Queue up/down ordering | Fixed | Sends lowercase `up`/`down` to match Rust-generated `QueueDirection`. |
| Double-click completed download | Works | Opens file; otherwise opens Properties. |
| Status/progress/speed/ETA updates | Works | Store listener handles progress/state events; existing media-size finalization behavior is preserved. |
| Column resize/layout | Works | Local UI-only state; no backend side effect. |

### Add Download window

| Surface/action | Status after review | Notes |
| --- | --- | --- |
| URL/multiple URL input | Works | Lines are parsed and invalid URLs surface per-item error state. |
| Paste/deep-link/extension prefill | Works | App-level paste/deep-link/extension handlers open the modal with URLs. |
| Metadata detection | Works | HTTP metadata and media metadata go through IPC. |
| Refresh Metadata | Fixed | Button now re-runs the existing parser/metadata flow. |
| Media format selection | Works | Selected format selector/ext is applied before enqueue. |
| File name/category/destination detection | Works | Category-specific destination routing is preserved. |
| Destination folder selection | Works | Uses Tauri directory picker. |
| Auth/site-login credentials | Works | Keychain-backed site-login password lookup is preserved. |
| Add paused / start queued | Works | Adds local item and enqueues when start is requested. |
| Duplicate URL conflict | Fixed | `Replace` is no longer offered for URL-only duplicates. |
| Duplicate file conflict | Works | Rename/replace/skip flows remain available for file conflicts. |
| Cancel/close | Works | Modal state is reset through store. |

### Download Properties

| Surface/action | Status after review | Notes |
| --- | --- | --- |
| Opens for queued/downloading/paused/failed/completed | Works | Selected item is read live from store. |
| Live progress/speed/ETA/size summary | Works | Summary uses current store item, not stale initial-only form state. |
| Save editable queued/paused/failed settings | Works | Updates local persisted item used by resume/redownload/enqueue paths. |
| Completed download settings for redownload | Works | Identity remains read-only; transfer settings can be saved for redownload. |
| Active-transfer speed controls | Fixed | Controls are locked while transfer is active; copy now states that current backend options remain active until pause/stop. |
| Browse destination | Works when not locked | Uses directory picker. |

### Settings

| Tab/action | Status after review | Notes |
| --- | --- | --- |
| Downloads tab settings | Fixed/Works | Bare global speed-limit values now normalize as KiB/s for live backend calls and newly queued downloads. |
| Look and feel tab | Works | Theme, font size, row density, dock badge, menu bar icon persist and drive app effects. |
| Network tab | Works | Proxy mode/host/port/user-agent values are used for enqueue payloads. |
| Locations tab | Works | Default/category paths persist; bulk category creation calls backend. |
| Site Logins tab | Works | Add/remove uses settings plus keychain password storage/deletion. |
| Power tab | Works | Prevent-sleep setting drives backend keep-awake effect. |
| Engine tab diagnostics | Works | Recheck calls registered engine status commands and surfaces errors/details. |
| Integrations tab | Works | Token copy/regenerate and extension pairing token updates are wired; token remains keychain-backed. |
| About tab updates | Works | Check Now calls update check and surfaces result via toast. |

### Menus/context menus/dialog buttons

| Surface/action | Status after review | Notes |
| --- | --- | --- |
| Download row context menu | Fixed/Works | Copy actions now resolve correctly and surface errors; start/pause/remove/redownload/properties remain wired. |
| Sidebar queue context menu | Works | Start/pause/rename/remove queues are wired through store. |
| Delete confirmation dialog | Works | Remove-only and remove+trash paths preserved. |
| Duplicate resolution dialog | Fixed | Replacement is now file-conflict-only. |

### Backend IPC coverage

| Check | Status |
| --- | --- |
| Frontend IPC command names exist in Rust registration | Works |
| Queue direction argument casing | Fixed |
| Global speed-limit unit handling | Fixed in frontend and backend startup/live command handling |
| File open/reveal/trash path authorization | Preserved |
| Generic duplicate-check/delete path guards | Preserved; no security loosening introduced |

## Not fixed / follow-up

- Full packaged GUI/manual click-through was not launched in this pass; validation was code-trace plus `npm run build` and Rust tests.
- Existing generic `check_file_exists` / `delete_file` remain scoped by `is_safe_path`, but they are broader than ownership-based open/reveal/trash commands. I did not loosen them. A future hardening pass should replace duplicate-file replacement with an explicit user-selected-destination trash command or ownership-aware pre-registration flow.
- Per-download speed changes for already-running transfers are not implemented because the current backend has no active-transfer per-item speed-limit IPC. The UI now stops promising that behavior.

## Validation

- `npm run build`: pass
- `cd src-tauri && cargo test --quiet`: pass

