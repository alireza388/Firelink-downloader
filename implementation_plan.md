# Firefox Offline Handoff and Metadata Fallback Plan

## Constraints

- No Native Messaging, helper executable, background service, or Apple Developer account dependency.
- Firefox controls first-use external-protocol confirmation. Firelink cannot suppress it.
- First use requires user approval; Firefox may offer “Always allow this extension to open firelink links.”
- Automatic browser-download capture never launches Firelink and resumes Firefox unless `/download` confirms acceptance.

## 1. Firefox launch-and-reconnect flow

1. Add launch-only `firelink://launch`.
2. Manual action first sends original authenticated `/download` payload.
3. Launch fallback is allowed only when authenticated discovery finds no Firelink server.
   - Never launch for `403`, other `4xx`, or a Firelink server returning `5xx`.
   - A startup `503` may be retried within the existing startup deadline.
   - Never retry an ambiguous POST failure after request transmission because delivery may already have happened.
4. Offline manual actions use one shared launch operation:
   - Create one inactive `firelink://launch` tab.
   - Queue immutable original payloads while startup is in progress.
   - Poll authenticated `/ping` across `127.0.0.1:6412-6422` with bounded retries.
   - Deliver every queued payload exactly once after startup.
   - Close the launch tab only after all queued payloads reach a terminal result.
5. Success means authenticated `/download` returned success. Tab creation is never success.
6. Timeout/cancel:
   - Stop after defined deadline.
   - Close temporary tab where Firefox permits.
   - Notify that Firelink was not opened and no download was added.
7. Store consecutive launch-timeout count and cooldown timestamp in `chrome.storage.local`.
   - Reset both after successful startup delivery.
   - During cooldown, do not open another protocol tab; show concise troubleshooting guidance.
8. Invalid pairing tokens never trigger protocol fallback.

## 2. Firefox first-use guidance

- README and popup explain first-use Firefox confirmation and “Always allow.”
- State clearly Firelink cannot bypass browser-controlled prompt.
- Repeated timeout guidance suggests confirming protocol permission, opening Firelink once, and checking installation.

## 3. Typed desktop deep links

Use a typed parser result:

- `Launch`
- `Add(Vec<String>)`
- `Invalid`

Rules:

- `firelink://launch` must contain exact scheme and host, with no username, password, port, path beyond `/`, query, or fragment.
- `Launch` restores/focuses window and never enters `CaptureUrls`.
- `firelink://add?url=...` keeps current external integration behavior.
- Unsupported schemes/hosts and malformed nested URLs return `Invalid`.
- Startup and already-running links use same dispatch function.
- Add URLs remain buffered in `DownloadCoordinator` until frontend listeners report ready.

## 4. Explicit metadata model

Each draft row has:

- Stable `id`
- Normalized `sourceUrl` identity
- `downloadUrl`, which metadata redirects may update
- Monotonic request `generation`
- Required status union: `loading | ready | metadata-error | invalid`
- Fallback filename and optional size bytes (`undefined` means unknown)
- Direct/media classification
- Optional successful metadata and selected media format

Malformed or unsupported URLs are `invalid`. Valid URLs whose metadata request fails remain `metadata-error`.

## 5. Parsing and enrichment separation

- Extract pure URL parsing/reconciliation helpers from modal.
- Normalize and deduplicate input by `sourceUrl`, preserving first occurrence order.
- Preserve existing rows, IDs, successful metadata, and selected formats.
- Create `loading` rows only for new valid URLs.
- Metadata results apply only when row ID, `sourceUrl`, and request generation still match.
- Save location, selection, and another row’s result never restart metadata.
- Credential edits affect transfer credentials and future failed-row retries only. Ready metadata remains unchanged.

## 6. Failed-only refresh

- Refresh selects only `metadata-error` rows.
- Increment only those rows’ generations and mark only those rows `loading`.
- Preserve every successful row and selected format.
- Failed retry restores fallback row as `metadata-error`.
- Disable refresh when no failed rows exist.

## 7. Submission eligibility and fallback routing

Eligible: `ready`, `metadata-error`.

Blocking: `loading`, `invalid`.

- Start Downloads and Add to Queue share one eligibility helper.
- Unknown size stays `undefined`; display text is derived and `"Unknown"` is not persisted as size.
- Failed direct rows retain original URL, fallback filename, direct routing, credentials, mirrors, duplicate checks, and destination logic.
- Failed media rows retain `isMedia: true`, send no format selector, and use yt-dlp default selection.
- Remove redownload validation requiring a media selector; selector-less media items remain valid.

## 8. Add-window messaging

- Loading: “Waiting for metadata for N downloads.”
- Mixed: “N downloads ready; M will use fallback filename and unknown size.”
- All failed: “Metadata is unavailable. Downloads can still be added using fallback details.”
- Invalid: “Correct or remove N invalid URL(s) before continuing.”
- Failed rows remain visually distinct but usable.

## 9. Tests

### Extension

- Offline manual action opens `firelink://launch`, not `firelink://add`.
- Tab creation is not success.
- Authenticated discovery retries; original payload fields survive startup.
- Shared startup sends concurrent payloads once and closes tab after terminal delivery.
- Timeout/cancel notifies failure and records cooldown.
- Automatic capture never opens protocol tab.
- Invalid token and other server errors never launch.
- Ambiguous POST failure never resends.

### Desktop

- Exact launch restores without downloads.
- Add links still parse.
- Unsupported/malformed links reject.
- Startup/running dispatch match.
- Startup Add URLs wait for frontend readiness.

### Add window

- Pure helper tests cover reconciliation, stale generations, failed-only refresh, eligibility, fallback routing, unknown size, duplicate filenames, and destinations.
- Component-level behavior uses pure helpers where possible; no DOM test runtime is required unless interaction coverage cannot be expressed through helpers.

## 10. Acceptance and verification

Run:

```bash
npm run test
npm run build
(cd Extensions/Firefox && npm run check)
(cd src-tauri && cargo check)
(cd src-tauri && cargo test --all-targets)
```

Then perform Firefox offline and mixed/all-failed metadata acceptance scenarios from original plan.
