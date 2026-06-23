# Firelink Release Process

Targets:

- macOS arm64 DMG
- Windows x64 NSIS installer
- Linux x64 AppImage

## Distribution policy

Firelink does not use an Apple Developer account. macOS releases are unsigned and not notarized. Users must explicitly approve the downloaded app through Finder or macOS Privacy & Security. Release copy must never describe these builds as signed, notarized, or Gatekeeper-approved.

Windows releases are currently unsigned. SmartScreen may warn until code signing is added.

## Engine supply chain

Firelink never falls back to system-installed media tools.

- `engines.lock.json` pins current committed macOS payload hashes.
- `engine-sources.lock.json` pins Windows/Linux source archives and checksums.
- `scripts/provision-engines.js` downloads and verifies target archives.
- `scripts/stage-engines.js` creates one target-specific bundle payload.
- `scripts/verify-binaries.js` runs architecture, packaging, version, and RPC checks.

yt-dlp must remain its official PyInstaller **onedir** distribution: launcher plus adjacent `_internal` runtime. Onefile builds are rejected because repeated extraction caused roughly 17-second startup latency.

## Version update

Keep versions aligned:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

## Local macOS build

```bash
npm ci
node scripts/stage-engines.js --target aarch64-apple-darwin
node scripts/verify-binaries.js --staged --target aarch64-apple-darwin
npm test -- --run
npm run build
cd src-tauri && cargo test --all-targets
cd ..
npm run tauri build -- --target aarch64-apple-darwin --bundles dmg
```

Verify packaged resources, then launch outside repository working directory:

```bash
APP="src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Firelink.app"
node scripts/verify-binaries.js --search-root "$APP" --target aarch64-apple-darwin
node scripts/smoke-packaged-app.js --executable "$APP/Contents/MacOS/firelink"
```

## Automated release

Push a version tag:

```bash
git tag v<version>
git push origin v<version>
```

GitHub Actions builds all targets on native runners, verifies engines inside final package contents, performs packaged launch smoke where supported, creates SHA-256 sums, then publishes one GitHub Release.

No target may silently skip missing engines, failed extraction, checksum mismatch, or missing package output.
