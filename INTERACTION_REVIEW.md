# Firelink UI Interaction Inventory

## 1. Main Window & Sidebar

### Sidebar Actions
- **Select Filter/Category/Queue**
  - **Location**: Sidebar item clicks (`NavItem`, `QueueItem`).
  - **Component**: `Sidebar.tsx` (`onSelectFilter`)
  - **Store Action**: `useSettingsStore.setActiveView('downloads')`, updates local state filter.
  - **Expected**: Main table filters to show matching downloads.
- **Add Queue**
  - **Location**: Sidebar '+' button.
  - **Component**: `Sidebar.tsx` (`handleAddQueueSubmit`)
  - **Store Action**: `useDownloadStore.addQueue()`
  - **Expected**: A new custom queue is created.
- **Queue Context Menu**
  - **Location**: Right-click on a custom queue.
  - **Component**: `Sidebar.tsx` (`handleQueueContextMenu`)
  - **Actions**:
    - **Start Queue**: `useDownloadStore.startQueue()` -> IPC `start_download` (for active items)
    - **Pause Queue**: `useDownloadStore.pauseQueue()` -> IPC `pause_download`
    - **Rename Queue**: `useDownloadStore.renameQueue()`
    - **Delete Queue**: `useDownloadStore.removeQueue()` -> IPC `db_delete_queue`
  - **Expected**: Context menu opens, performing the respective queue manipulation.
- **Resize Sidebar**
  - **Location**: Sidebar drag handle.
  - **Component**: `App.tsx` (`startSidebarResize`)
  - **Expected**: Drags and resizes the sidebar width dynamically.

### Main Toolbar
- **Toggle Sidebar**
  - **Location**: Top-left icon (when sidebar is hidden).
  - **Component**: `DownloadTable.tsx` / `Sidebar.tsx`
  - **Store Action**: `useSettingsStore.toggleSidebar()`
- **Add Download**
  - **Location**: '+' Button.
  - **Component**: `DownloadTable.tsx`
  - **Store Action**: `useDownloadStore.toggleAddModal(true)`
- **Resume All / Pause All**
  - **Location**: Toolbar Play/Pause icons.
  - **Component**: `DownloadTable.tsx`
  - **Store Action**: `handleResume` -> `resumeDownload`, `handlePause` -> IPC `pause_download`
  - **Rust**: `commands::pause_download`
  - **Expected**: Iterates through visible items and pauses/resumes them.

## 2. Download Table

- **Table Column Resize**
  - **Location**: Table headers drag handle.
  - **Component**: `DownloadTable.tsx` (`startColumnResize`)
  - **Expected**: Modifies local state grid template widths.
- **Row Double Click**
  - **Location**: Table row.
  - **Component**: `DownloadTable.tsx` (`handleDownloadDoubleClick`)
  - **Expected**: If completed, opens file (IPC `open_downloaded_file`). Otherwise, opens properties (`useDownloadStore.setSelectedPropertiesDownloadId`).
- **Row Context Menu (Right Click)**
  - **Location**: Table row.
  - **Component**: `DownloadTable.tsx`
  - **Actions**:
    - **Open**: IPC `open_downloaded_file` -> Rust `commands::open_downloaded_file`
    - **Show in Finder**: IPC `reveal_in_file_manager` -> Rust `commands::reveal_in_file_manager`
    - **Pause**: IPC `pause_download` -> Rust `commands::pause_download`
    - **Resume**: `useDownloadStore.resumeDownload()` -> IPC `resume_download` -> Rust `commands::resume_download`
    - **Redownload**: `useDownloadStore.redownload()` -> IPC `remove_download` -> Rust `commands::enqueue_download`
    - **Copy Address**: `navigator.clipboard.writeText(url)`
    - **Copy File Path**: `navigator.clipboard.writeText(fullPath)`
    - **Remove**: `useDownloadStore.openDeleteModal()`
    - **Properties**: `useDownloadStore.setSelectedPropertiesDownloadId()`

## 3. Add Downloads Window

- **Input URLs**
  - **Location**: Main textarea.
  - **Component**: `AddDownloadsModal.tsx`
  - **Expected**: Parses text, fetches metadata (IPC `fetch_metadata` or `fetchMediaMetadataDeduped`), and populates preview list.
- **Refresh Metadata**
  - **Location**: "Refresh Metadata" button.
  - **Component**: `AddDownloadsModal.tsx`
  - **Expected**: Re-triggers metadata fetch for URLs.
- **Preview List Selection**
  - **Location**: Preview items list.
  - **Component**: `AddDownloadsModal.tsx` (`setSelectedItemIndex`)
  - **Expected**: Updates the right-side form to reflect the selected item's specific options.
- **Media Format Selection**
  - **Location**: Format list (if media).
  - **Component**: `AddDownloadsModal.tsx` (`selectMediaFormat`)
  - **Expected**: Updates the selected format, updating estimated sizes and file extensions.
- **Browse Save Location**
  - **Location**: "Select" button next to save location.
  - **Component**: `AddDownloadsModal.tsx` (`handleBrowse`)
  - **Expected**: Opens Tauri native directory picker.
- **Options Toggles & Inputs**
  - **Location**: Right-side pane.
  - **Actions**: Change connections, toggle speed limit, set username/password, expand advanced, toggle checksum, edit headers/cookies/mirrors.
  - **Expected**: Mutates local component state for the download configuration.
- **Start / Add to Queue**
  - **Location**: Bottom right buttons.
  - **Component**: `AddDownloadsModal.tsx` (`handleStart`)
  - **Store Action**: `useDownloadStore.addDownload()`
  - **IPC Command**: `enqueue_download` (called by store)
  - **Rust**: `commands::enqueue_download`
  - **Validation**: Checks for duplicates on disk (IPC `check_file_exists`) and in store, potentially opening `DuplicateResolutionModal`.

## 4. Download Properties Window

- **Browse Save Location**
  - **Location**: "Select" button.
  - **Component**: `PropertiesModal.tsx`
  - **Expected**: Opens Tauri native directory picker.
- **Form Inputs**
  - **Location**: Text inputs and selects.
  - **Component**: `PropertiesModal.tsx`
  - **Expected**: Adjusts URL, filename, connections, speed limits, auth (matches/custom/none), advanced settings. Disabled if the download is locked/active.
- **Save Changes**
  - **Location**: "Save" button.
  - **Component**: `PropertiesModal.tsx`
  - **Store Action**: `useDownloadStore.updateDownload()`
  - **Expected**: Validates inputs, saves to store, syncs to DB.

## 5. Settings Window

- **Tab Navigation**
  - **Location**: Top horizontal tabs.
  - **Store Action**: `useSettingsStore.setActiveSettingsTab()`
- **Downloads Settings**
  - **Location**: "Downloads" tab.
  - **Actions**: Change default connections, parallel downloads (IPC `set_concurrent_limit`), global speed limit (IPC `set_global_speed_limit`), max retries, notification toggles.
- **Look and feel**
  - **Location**: "Look and feel" tab.
  - **Actions**: Theme switch, font size, density, dock badge toggle (IPC `update_dock_badge`), menu bar icon (IPC `toggle_tray_icon`).
- **Network**
  - **Location**: "Network" tab.
  - **Actions**: Change proxy mode/host/port, custom user agent.
- **Locations**
  - **Location**: "Locations" tab.
  - **Actions**:
    - Browse Default Path
    - Toggle "Ask where to save"
    - Browse "All Categories Base" -> Tauri Picker -> IPC `create_category_directories` -> Rust `commands::create_category_directories`
    - Browse specific category paths.
    - Reset Defaults: `useSettingsStore.resetCategoryDirectories()`
- **Site Logins**
  - **Location**: "Site Logins" tab.
  - **Actions**:
    - Add Login: IPC `set_keychain_password` -> `useSettingsStore.addSiteLogin()`
    - Delete Login (Trash icon): IPC `delete_keychain_password` -> `useSettingsStore.removeSiteLogin()`
- **Engine**
  - **Location**: "Engine" tab (when mounted).
  - **Component**: `SettingsView.tsx` (`runEngineChecks`)
  - **IPC**: `get_aria2_engine_status`, `get_ytdlp_engine_status`, `get_ffmpeg_engine_status`, `get_deno_engine_status`.
  - **Expected**: Polls backend for binary status and updates UI. Re-triggered with "Refresh" button.

## 6. Other Modals & Actions

- **Delete Confirmation Modal**
  - **Location**: Prompt when removing a download.
  - **Component**: `DeleteConfirmationModal.tsx`
  - **Actions**:
    - Remove from list: `useDownloadStore.removeDownload()` -> IPC `db_delete_download`
    - Delete file & remove: `useDownloadStore.removeDownload()` + IPC `trash_download_assets` -> Rust `commands::trash_download_assets`
- **Duplicate Resolution Modal**
  - **Location**: Triggered when adding a download that already exists.
  - **Component**: `DuplicateResolutionModal.tsx`
  - **Actions**: Select "Rename", "Replace", or "Skip".
  - **Expected**: Modifies the batch of downloads added via `executeAddDownloads`.
- **Quality Modal**
  - **Location**: Triggered when an active media download requires format resolution.
  - **Component**: `QualityModal.tsx`
  - **Expected**: Approves formats via local store or cancels metadata request.
- **Global Keyboard Shortcuts**
  - **Location**: `App.tsx` (paste event listener)
  - **Expected**: Pasting URLs anywhere outside inputs triggers `useDownloadStore.openAddModalWithUrls(text)`.
- **System Sleep / Power**
  - **Location**: "Power" tab.
  - **Expected**: Interacts with IPC `set_prevent_sleep` -> Rust `commands::set_prevent_sleep`.

## Likely Validation Methods
- **UI State**: Checking Zustand store (`useDownloadStore`, `useSettingsStore`) and React DevTools.
- **Rust Backend**: Monitoring standard output/stderr for `Invoke command` errors, and inspecting `src-tauri/src/commands.rs`.
- **File System**: Checking disk for queue persistence and newly created directories (e.g. `create_category_directories`).
