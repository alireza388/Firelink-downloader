# Headless Download Tests

Run the full Rust suite:

```sh
cd src-tauri
cargo test --all-targets
```

Run the queue-manager harness when changing aria2 scheduling, concurrency, and
retry behavior:

```sh
cd src-tauri
cargo test --test queue_manager -- --nocapture
```

Run the media metadata smoke test with an explicit URL when changing yt-dlp
integration:

```sh
cd src-tauri
FIRELINK_LIVE_YOUTUBE_URL='https://www.youtube.com/watch?v=dQw4w9WgXcQ' \
  cargo test filters_live_youtube_metadata_from_env --lib -- --ignored --nocapture
```
