# Firelink Release Process

## Prerequisites

- macOS ARM64 (aarch64) build host
- Node.js 22+
- Rust toolchain
- Tauri CLI (`cargo install tauri-cli --version "^2"`)
- Apple Developer account with signing certificates (for notarized builds)

## Step-by-step

### 1. Update version

Bump version in these files so they match:

| File | Field |
|------|-------|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `package.version` |
| `src-tauri/tauri.conf.json` | `version` |

### 2. Verify engines locally

```bash
node scripts/verify-binaries.js
```

This runs all pre-release checks:
1. Target-triple sidecars exist
2. Binaries are executable
3. `file(1)` identifies correct architecture
4. `otool -L` shows no local-only dylib paths
5. No `/opt/homebrew` or `/usr/local/Cellar` linkage
6. yt-dlp packaging is intact (onedir or standalone)
7. Every engine runs and reports its version
8. aria2 RPC daemon starts and responds to JSON-RPC
9. No forbidden stderr patterns (`Library not loaded`, etc.)

The build is **blocked** if any check fails, enforced via `beforeBuildCommand` in `tauri.conf.json`.

### 3. Build

```bash
npm run tauri build
```

### 4. Build artifacts

The packaged `.app` and `.dmg` appear in:

```
src-tauri/target/release/bundle/macos/
```

### 5. GitHub Release

Push a tag to trigger the release workflow:

```bash
git tag v<version>
git push origin v<version>
```

The release workflow (`.github/workflows/release.yml`) will:

| Job | What it does |
|-----|-------------|
| `engine-verification` | Runs `verify-binaries.js`, builds `.app`, uploads artifacts |
| `create-release` | Creates GitHub Release with checksums and release notes |

## CI verification

Every PR and push to `main` also runs `node scripts/verify-binaries.js` (see `.github/workflows/ci.yml`), so broken engines are caught before they reach a release tag.
