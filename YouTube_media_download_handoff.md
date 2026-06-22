# YouTube and Media Download Handoff

## Repository state

The YouTube and media fixes were committed and pushed in:

- `a8b7920 fix(media): harden YouTube metadata loading`
- `340ef09 fix(media): correct YouTube size estimates`
- `226c791 fix(media): correct HLS progress tracking`

Do not discard unrelated worktree changes while working in this area.

## Non-negotiable architecture

### Keep the self-contained yt-dlp onedir distribution

Do not replace the bundled onedir distribution with yt-dlp onefile or a system
yt-dlp installation.

The onefile executable incurred roughly 17 seconds of extraction and startup
latency. Users cannot be expected to have Python or yt-dlp installed or
available through `PATH`.

The packaged layout must contain:

- `yt-dlp-<target>`
- The adjacent `_internal/` directory
- The embedded Python runtime
- The `yt_dlp_ejs` solver files

`scripts/verify-binaries.js` enforces this layout. Cross-platform builds must
provide an equivalent onedir distribution for each target. Do not implement
cross-platform support by falling back to a user-managed `PATH`.

### Keep metadata loading deterministic

The metadata implementation in `src-tauri/src/lib.rs` deliberately:

- Resolves only the bundled yt-dlp executable.
- Passes bundled Deno and FFmpeg by absolute path.
- Uses a minimal system `PATH`.
- Uses `--skip-download`.
- Requests only `title`, `duration`, `thumbnail`, and `formats`.
- Includes the cookie browser and credentials in the cache key.
- Deduplicates concurrent identical requests.
- Caches successful metadata for 60 seconds.
- Limits the cache to 128 entries.

Frontend request deduplication also exists in
`src/utils/mediaMetadata.ts`. Do not remove either deduplication layer. React
development behavior can otherwise start duplicate yt-dlp processes.

Deno is a JavaScript runtime used by yt-dlp extractors. It is not the metadata
engine.

### Keep media-format interpretation in the backend

`build_media_format_options` in `src-tauri/src/lib.rs` is the source of truth.
Do not reintroduce a separate format parser in `AddDownloadsModal.tsx`.
Duplicating this logic caused format selection and size estimation to drift.

Required invariants:

- Do not add a synthetic `Best` option.
- Exclude storyboards, MHTML, thumbnails, subtitles, and non-media entries.
- Match resolutions exactly. A 1080p stream must never produce a 1440p row.
- Bind each displayed option to concrete yt-dlp stream IDs, such as `301+251`.
- MKV, MP4, and WebM options must describe the streams actually selected.
- Do not add a separate audio stream when the selected video already has audio.

### Keep size values honest

Size semantics:

- `filesize` is exact.
- `filesize_approx` is approximate.
- When yt-dlp provides neither, estimate from bitrate multiplied by duration
  and mark the result approximate.
- For split formats, combine the video and audio sizes.
- Approximate sizes in the UI must have a `~` prefix.
- Temporary component-stream totals must not overwrite the download row's
  estimate.
- Successful completion must replace the estimate with the actual output file
  size read from disk.
- Stores may treat a progress size as authoritative only when
  `size_is_final` is true.

The primary UI consumers are:

- `src/components/AddDownloadsModal.tsx`
- `src/components/QualityModal.tsx`

### Do not trust temporary HLS byte totals for progress

yt-dlp can initially emit data similar to:

```text
downloaded_bytes=1024
total_bytes_estimate=1024
fragment_index=0
fragment_count=354
_percent_str=100.0%
```

This does not mean the download is complete. It represents an early HLS
fragment with a temporary size estimate.

`parse_media_progress_line` must therefore prefer:

```text
fragment_index / fragment_count
```

when fragment information is available.

For separate video and audio streams, `aggregate_media_fraction` advances to
the next track only when the previous track was effectively complete and the
new track restarts near zero. Do not restore the old heuristic that treated any
large percentage drop as a track transition.

Visible speed is derived from downloaded-byte deltas where possible. Raw
yt-dlp speed is only a fallback.

## Required regression checks

Before accepting changes to yt-dlp arguments, binary packaging, metadata,
progress, sizes, format selection, pause/resume, or the media UI, run:

```bash
cargo test --all-targets
npm run build
npm test -- --run
node scripts/verify-binaries.js
git diff --check
```

Run the live format smoke with a currently available multi-quality video:

```bash
FIRELINK_LIVE_YOUTUBE_URL='https://www.youtube.com/watch?v=<id>' \
cargo test filters_live_youtube_metadata_from_env --lib -- --ignored --nocapture
```

Preserve these Rust regression tests:

- `builds_compact_media_options_without_storyboards`
- `estimates_missing_video_size_from_bitrate_and_uses_exact_stream_ids`
- `uses_fragment_progress_instead_of_temporary_hls_size_estimates`
- `advances_tracks_only_after_a_completed_track_restarts`
- `derives_main_window_speed_from_downloaded_byte_delta`
- The structured yt-dlp, aria2, and legacy progress parser tests

## Manual packaged-app checks

Test the packaged application, not only development mode:

1. Launch it outside the repository working directory.
2. Select a browser cookie source.
3. Paste a YouTube URL into Add Downloads.
4. Confirm media formats appear within the expected warm-start budget, with a
   target below eight seconds.
5. Confirm there is no `Best` row.
6. Confirm each displayed quality matches its actual resolution.
7. Confirm approximate sizes include `~`.
8. Start a split-stream or HLS download.
9. Confirm progress increases gradually instead of immediately reaching 100%.
10. Confirm the final displayed size equals the completed file's size on disk.
11. Pause and resume, confirming that the selected stream IDs and estimated
    size remain intact.

Do not treat a deleted, private, age-restricted, or geographically unavailable
test video as proof of a product regression. Reproduce the problem with another
public multi-quality video first.
