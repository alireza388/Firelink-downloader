# Headless Download Engine Tests

Run the full Rust suite:

```sh
cd apps/desktop/src-tauri
cargo test --all-targets
```

Run only the async download integration harness with deterministic serial
performance measurements and visible test output:

```sh
cd apps/desktop/src-tauri
RUST_BACKTRACE=1 cargo test --test download_engine -- --test-threads=1 --nocapture
```

The harness binds an ephemeral loopback port and requires no GUI, external
network access, or bundled media binaries. It validates:

- aggregation of many streamed HTTP body chunks;
- pause and ranged resume through `DownloadCoordinator`;
- cancellation and partial-file cleanup;
- SHA-256 integrity after resume;
- retry recovery from transient HTTP failures;
- terminal error reporting after the retry budget is exhausted;
- a five-second local transfer performance budget for a 3 MiB fixture.
