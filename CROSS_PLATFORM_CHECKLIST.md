# Firelink Cross-Platform Release Checklist

Audit date: 2026-06-23

Targets:

- macOS arm64
- Windows x64
- Linux x64 AppImage

## Current status

| Target | Implementation | Validation |
|---|---|---|
| macOS arm64 | Complete | Native build, packaged-engine verification, and packaged launch smoke passed |
| Windows x64 | Complete | Payload provision/static verification passed; native CI and clean-machine QA pending |
| Linux x64 AppImage | Complete | Payload provision/static verification passed; native CI and desktop-matrix QA pending |

Windows/Linux publication remains blocked until native GitHub Actions and clean-machine QA pass. macOS distribution is intentionally unsigned and unnotarized because no Apple Developer account is planned.

## Implemented foundations

- Central target triple, executable suffix, trusted system `PATH`, engine naming, and path-comparison helpers.
- Target-aware engine resolver for development and packaged resources.
- Absolute yt-dlp downloader/tool paths. Unix symlink staging removed.
- Official PyInstaller **onedir** yt-dlp payloads retained for every target.
- Checksum-pinned source archives, payload manifests, target-only staging, and packaged-resource verification.
- macOS Keychain, Windows Credential Manager, and Linux Secret Service keyring backends.
- Session-only browser pairing fallback when native credential storage is unavailable.
- OS standard directories plus synchronously persisted user-approved download roots.
- Windows reserved filename sanitization and platform-correct duplicate path comparison.
- Platform-specific Tauri window and bundle configuration.
- Native CI matrix and release jobs for unsigned macOS DMG, unsigned Windows NSIS, and Linux AppImage.
- Linux deep-link registration without native messaging.
- Platform-aware scheduler permissions, tray/menu labels, dock badge, notifications, proxy behavior, and sleep prevention.
- Cancellable delayed post-queue system actions with active-transfer recheck.
- Bounded local logging enabled by default, secret/home-path redaction, and safe export naming.
- Third-party notices and engine provenance locks included in packages.

## Engine payloads

Required names:

```text
macOS arm64
aria2c-aarch64-apple-darwin
yt-dlp-aarch64-apple-darwin
ffmpeg-aarch64-apple-darwin
deno-aarch64-apple-darwin

Windows x64
aria2c-x86_64-pc-windows-msvc.exe
yt-dlp-x86_64-pc-windows-msvc.exe
ffmpeg-x86_64-pc-windows-msvc.exe
deno-x86_64-pc-windows-msvc.exe

Linux x64
aria2c-x86_64-unknown-linux-gnu
yt-dlp-x86_64-unknown-linux-gnu
ffmpeg-x86_64-unknown-linux-gnu
deno-x86_64-unknown-linux-gnu
```

Supply-chain files:

- `engines.lock.json`: committed macOS payload hashes.
- `engine-sources.lock.json`: Windows/Linux archive URLs and hashes.
- `scripts/provision-engines.js`: download, checksum, extract, normalize, and manifest.
- `scripts/stage-engines.js`: verify and stage one target.
- `scripts/verify-binaries.js`: architecture, runtime layout, linkage, version, startup, and aria2 RPC checks.

yt-dlp must remain launcher plus adjacent `_internal`. Onefile builds are rejected. Warm startup target remains below eight seconds; current macOS warm `--version` measured about 0.23 seconds.

## Filesystem and permissions

- Download authorization uses canonical paths and approved roots; no hardcoded `/Volumes`.
- Folder-dialog selections are approved synchronously in backend before enqueue, avoiding settings-persistence races.
- Open, reveal, replace, and delete operations remain constrained to Firelink-owned paths.
- `~`, Windows separators, missing leaf components, symlinks, and case rules are handled per platform.
- Scheduler automation permission controls appear only on macOS. Windows/Linux show honest system-policy behavior.
- Sleep prevention uses platform backend behavior and surfaces errors.
- Browser pairing survives credential-store failure only for current session and reports that state.

## Desktop integration

- macOS: dock badge, menu-bar wording, transparent sidebar window, unsigned/unnotarized release.
- Windows: system tray wording, Mica window config, NSIS installer, SmartScreen warning expected while unsigned.
- Linux: system tray wording, opaque decorated window, AppImage, runtime deep-link registration.
- Notifications request permission and surface denial/errors. Sound names are platform-specific where verified.
- Post-queue sleep/shutdown/restart waits ten seconds, can be cancelled, and aborts if transfers resume.

## Logging and privacy

- Logging starts enabled and rotates at 10 MB with three retained files.
- Authorization, cookies, signed URL queries, tokens, and home paths are redacted.
- Export avoids exposing source log directory paths.
- Logs remain local unless user explicitly exports them.

## Browser integration

- Existing authenticated loopback HTTP integration remains.
- Responses identify Firelink through `X-Firelink-Server`.
- Pairing tokens use native credential storage where available.
- No native-messaging dependency is introduced.

## Validation completed

- Frontend: 31 tests passed.
- Rust: 82 unit tests passed, 1 network-dependent test ignored.
- Download engine: 5 integration tests passed.
- Queue manager: 17 integration tests passed.
- TypeScript/Vite production build passed.
- Rust/TypeScript binding generation passed.
- Windows and Linux payload provisioning plus static architecture/runtime-layout verification passed.
- macOS target staging, engine runtime/RPC verification, release `.app` build, packaged-resource verification, notice layout, and outside-repository launch smoke passed.
- Workflow YAML parsing and `git diff --check` passed.

## Native QA still required

### Windows x64

- Run CI/release jobs on `windows-latest`.
- Install NSIS output on clean Windows 11 x64.
- Verify SmartScreen flow, tray, notifications, sleep prevention, file dialogs, path case behavior, Credential Manager persistence, browser handoff, media download, pause/resume, replace/delete, scheduler, and uninstall.

### Linux x64

- Run CI/release jobs on Ubuntu 22.04.
- Launch extracted AppImage under X11 and Wayland desktops.
- Verify Secret Service present and absent behavior, tray support variance, notifications, sleep inhibition, file dialogs, deep links, browser handoff, media download, pause/resume, replace/delete, scheduler, and AppImage portability.

### macOS arm64

- Test downloaded unsigned artifact on a clean machine.
- Confirm documented Finder/Privacy & Security approval flow.
- Verify first-launch unsigned-engine delay, notifications, menu bar, sleep prevention, scheduler automation permission, browser handoff, and media download.

## Release decision

Implementation phase is complete. Release certification is not complete until native Windows/Linux workflows and clean-machine QA pass. Failures found there must be fixed at root before publication.
