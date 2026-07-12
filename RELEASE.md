# Firelink Release Process

Targets:

- macOS arm64 DMG
- Windows x64 NSIS installer
- Windows x64 portable ZIP
- Linux x64 AppImage
- Linux x64 Debian package
- Linux x64 RPM package

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

Linux `.deb` and `.rpm` packages are built with the complete verified engine payload. The AppImage is built separately with the engine payload temporarily omitted, then repacked from the verified payload because the AppImage tooling can rewrite bundled native binaries.

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

GitHub release publication is intentionally manual. Tag pushes build and upload
artifacts, but the `publish` job only runs from a `workflow_dispatch` on a `v*`
tag when both release-certification inputs are checked after Windows, Linux,
and macOS clean-machine QA.

## Automated release builds

Push a version tag to build and verify native artifacts:

```bash
git tag v<version>
git push origin v<version>
```

GitHub Actions builds all targets on native runners, verifies engines inside
final package contents, performs packaged launch smoke where supported, and
uploads artifacts. Publish the GitHub Release with a manual `workflow_dispatch`
run on the same tag after clean-machine QA is complete.

No target may silently skip missing engines, failed extraction, checksum mismatch, or missing package output.
