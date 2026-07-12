<div align="center">
  <img src="src/assets/app-icon.png" alt="Firelink" width="112" height="112" />

  # Firelink

  **A fast, focused desktop download manager for macOS, Windows, and Linux.**

  [![Version](https://img.shields.io/badge/version-1.0.4-6f42c1?style=flat-square)](https://github.com/nimbold/Firelink/releases)
  [![macOS](https://img.shields.io/badge/macOS-111111?style=flat-square&logo=apple&logoColor=white)](#platforms)
  [![Windows](.github/badges/windows.svg)](#platforms)
  [![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)](#platforms)
  [![License](https://img.shields.io/github/license/nimbold/Firelink?style=flat-square)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/nimbold/Firelink/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/nimbold/Firelink/actions/workflows/ci.yml)

  [Features](#features) · [Install](#installation) · [Browser Extension](#browser-extension) · [Development](#development) · [Release Notes](CHANGELOG.md)
</div>

<br/>

<div align="center">
  <img src="Screenshots/Dark%20theme%20-%20main.png" width="24%" alt="Firelink dark theme main window" />
  <img src="Screenshots/Dark%20theme%20-%20add%20window.png" width="24%" alt="Firelink dark theme add window" />
  <img src="Screenshots/Light%20theme%20-%20main.png" width="24%" alt="Firelink light theme main window" />
  <img src="Screenshots/Light%20theme%20-%20add%20window.png" width="24%" alt="Firelink light theme add window" />

  <details>
    <summary><b>View more screenshots</b></summary>
    <br/>
    <img src="Screenshots/Dark%20theme%20-%20settings.png" width="32%" alt="Firelink dark theme settings" />
    <img src="Screenshots/Light%20theme%20-%20settings.png" width="32%" alt="Firelink light theme settings" />
  </details>
</div>

## Why Firelink

Firelink is a desktop download manager for fast transfers, browser capture, media extraction, scheduling, and clear file placement.

It is now a cross-platform Rust/Tauri app with a React and TypeScript interface. A native backend coordinates downloads with aria2, yt-dlp, FFmpeg, Deno, and SQLite.

The current desktop release is **1.0.4**, paired with Firelink Companion **2.0.3**.

## Features

- **Segmented downloads** with aria2, retries, speed limits, and connection controls.
- **Media downloads** with yt-dlp, FFmpeg, Deno, live progress, speed, and ETA.
- **Add window** for metadata, duplicates, location choices, captured links, and clipboard-prefilled URLs.
- **Persistent queues** with pause, resume, retry, redownload, sorting, multi-select, and bulk actions.
- **Scheduling** with start/stop windows, speed rules, and post-queue actions.
- **File organization** with categories, default folders, a collapsible Folders section, per-download overrides, and reveal/trash actions.
- **Browser handoff** through local pairing, signed requests, Add window review, replay protection, and server checks.
- **Desktop integration** with tray controls, notifications, sounds, sleep prevention, and secure credential storage.
- **Diagnostics** with engine health checks, structured logs, and package verification.

## Installation

Download desktop builds from [GitHub Releases](https://github.com/nimbold/Firelink/releases).

| Platform | Package | Notes |
| --- | --- | --- |
| **macOS Apple silicon** | `.dmg` | Not notarized. If macOS blocks the first launch, approve Firelink in **System Settings -> Privacy & Security**. |
| **Windows x64** | NSIS `.exe` installer | Unsigned. Windows SmartScreen may warn until code signing is added. |
| **Windows x64 portable** | `.zip` archive | Extract to a writable folder and launch `firelink.exe`. App data stays under the archive's `data/` directory. |
| **Linux x64** | `.deb`, `.rpm`, or `.AppImage` | Use `.deb` for Debian-family systems, `.rpm` for Fedora/RPM-family systems, or AppImage as the portable fallback. AppImage may need executable permission. |

Bundles include the required engines. Users do not need aria2, yt-dlp, FFmpeg, Deno, Python, Homebrew, or another package manager.

The native packages use the distribution's normal desktop runtime dependencies. AppImage and the Windows portable ZIP are the portable distribution options for their respective platforms.

The Windows portable archive is an opt-in secondary distribution. Keep the extracted folder writable; `Program Files`, read-only media, and some network or synchronized folders can prevent SQLite and WebView data from being saved. Close Firelink before copying or moving the folder. Only one Firelink instance can run at a time, so close the installed app before launching the portable copy. Portable mode keeps application settings, queues, logs, and WebView data beside the executable. Credentials, browser cookies, and URL query/fragment data are not persisted in portable queue records; active downloads that depend on those URL components are marked failed and must be added again after restart. Saved site passwords remain in the Windows credential store and are intentionally not copied into the archive. The portable folder contains the extension pairing credential needed to preserve extension integration, so treat the folder as sensitive and do not share it. Saved absolute download locations may need to be selected again if the folder is moved to a different drive. The installer remains the supported path for `firelink://` browser launch registration.

## Browser Extension

<p align="center">
  <a href="https://addons.mozilla.org/en-US/firefox/addon/firelink-companion/"><img src="https://img.shields.io/badge/Install%20from-Firefox%20Add--ons-FF7139?style=for-the-badge&logo=firefox-browser&logoColor=white" alt="Install Firelink Companion from Firefox Add-ons" /></a>
  &nbsp;&nbsp;
  <a href="https://github.com/nimbold/Firelink-Extension#manual-chromium-installation"><img src="https://img.shields.io/badge/Manual%20install-Chromium-4285F4?style=for-the-badge&logo=googlechrome&logoColor=white" alt="Read manual Chromium install instructions" /></a>
</p>

Firelink Companion sends browser links and downloads to the desktop app.

What it adds:

- Automatic capture for regular browser downloads.
- Explicit Fetch media actions from the popup and page context menu.
- Context-menu actions for links and selected text.
- Firefox and Chromium support.
- Signed local requests using the token from **Settings -> Integrations**.
- Fallback to the browser download when Firelink is closed or rejects a handoff.
- Captured links always open Firelink's Add window before anything is added to the download list.

Install the extension, open Firelink, then pair it from **Settings -> Integrations**. Firefox users can install from Mozilla Add-ons. Chromium users can use the [manual load-unpacked flow](https://github.com/nimbold/Firelink-Extension#manual-chromium-installation) with `firelink-chromium.zip` from the [extension releases](https://github.com/nimbold/Firelink-Extension/releases). Firelink Companion 2.0.3 is the matching extension release for Firelink 1.0.4.

The extension lives in [Firelink-Extension](https://github.com/nimbold/Firelink-Extension). This repo also vendors it as the `Extensions/Browser` submodule.

## Platforms

| Target | Status |
| --- | --- |
| **macOS arm64** | Supported. Native build, engine checks, launch smoke test, ad-hoc-signed DMG workflow. |
| **Windows x64** | Supported. Native build, engine checks, silent installer smoke test, NSIS installer. |
| **Linux x64** | Supported. Native build, bundled-engine checks, package/AppImage launch smoke tests, `.deb`, `.rpm`, and AppImage. |

## Development

### Requirements

- Node.js 22 or newer
- npm
- Rust and Cargo
- [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/)

Clone the repository with its browser-extension submodule:

```sh
git clone --recurse-submodules https://github.com/nimbold/Firelink.git
cd Firelink
```

Install dependencies and launch the desktop app:

```sh
npm install
npm run tauri dev
```

Run the core checks:

```sh
node --test scripts/*.node-test.js
npm test -- --run
npm run build
cd src-tauri
cargo test --all-targets
```

Create a production bundle:

```sh
npm run tauri build
```

macOS uses locked payloads in `src-tauri/binaries`. Provision Windows and Linux payloads from checksum-pinned archives:

```sh
node scripts/provision-engines.js --target x86_64-pc-windows-msvc
node scripts/provision-engines.js --target x86_64-unknown-linux-gnu
```

Build staging includes only the current target. See `engines.lock.json`, `engine-sources.lock.json`, and [RELEASE.md](RELEASE.md).

## Repository Structure

```text
.
├── src/                  React and TypeScript interface
├── src-tauri/            Rust backend, Tauri config, and native tests
├── scripts/              Engine provisioning, release, and smoke-test tooling
└── Extensions/Browser/   Firelink Companion submodule
```

## Help and Project Status

- Report bugs or request improvements in [GitHub Issues](https://github.com/nimbold/Firelink/issues).
- Read [CHANGELOG.md](CHANGELOG.md) for release history.
- Review [RELEASE.md](RELEASE.md) for packaging policy and release verification.

## Technology & Credits

Firelink is made possible by these open-source projects:

- **[Tauri 2](https://tauri.app/)** for the lightweight desktop runtime
- **[Rust](https://www.rust-lang.org/)** and **[Tokio](https://tokio.rs/)** for native application logic
- **[React](https://react.dev/)** and **[TypeScript](https://www.typescriptlang.org/)** for the interface
- **[Zustand](https://zustand-demo.pmnd.rs/)** for frontend state management
- **[SQLite](https://www.sqlite.org/)** for persistent local data
- **[aria2](https://aria2.github.io/)** for segmented downloading
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**, **[FFmpeg](https://ffmpeg.org/)**, and **[Deno](https://deno.com/)** for media extraction and processing

## License

Firelink is available under the [MIT License](LICENSE).
