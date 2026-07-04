<div align="center">
  <img src="src/assets/app-icon.png" alt="Firelink" width="112" height="112" />

  # Firelink

  **A fast, focused desktop download manager for macOS, Windows, and Linux.**

  [![Version](https://img.shields.io/badge/version-1.0.1-6f42c1?style=flat-square)](https://github.com/nimbold/Firelink/releases)
  [![macOS](https://img.shields.io/badge/macOS-111111?style=flat-square&logo=apple&logoColor=white)](#platforms)
  [![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=windows11&logoColor=white)](#platforms)
  [![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)](#platforms)
  [![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
  [![Rust](https://img.shields.io/badge/Rust-backend-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)
  [![React](https://img.shields.io/badge/React-TypeScript-61DAFB?style=flat-square&logo=react&logoColor=111111)](https://react.dev/)
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

Firelink is built for people who want a real desktop download manager again: fast segmented transfers, browser capture, media extraction, scheduling, recovery, and clear control over where files land. Version 1.0.0 completes the move from the earlier macOS-only Swift app to a modern Rust/Tauri application with a React and TypeScript interface.

The app keeps the heavy work native. Downloads are coordinated by a Rust backend, accelerated with aria2, enriched with yt-dlp and FFmpeg for media workflows, and persisted locally with SQLite so queues survive restarts and app updates.

## Features

- **Fast segmented downloads** powered by aria2 with configurable connections, retries, and speed limits.
- **Media extraction** with yt-dlp, FFmpeg, and Deno for video/audio links and richer format selection.
- **A real Add window** for manual, extension-captured, and media downloads, including metadata, duplicate handling, and save-location choices before downloads start.
- **Persistent queue management** with safe concurrency limits, pause/resume, retry, redownload, sorting, multi-select, and bulk controls.
- **Download scheduling** with start/stop windows, speed-limiter tools, and optional post-queue actions.
- **Smart organization** through categories, default folders, per-download overrides, and open/reveal/trash actions.
- **Private browser handoff** through authenticated local pairing with replay protection and desktop-server proof checks.
- **Native desktop integration** including tray controls, notifications, completion sounds, sleep prevention, and OS keychain support where available.
- **Diagnostics built in** with engine health checks, structured logs, and packaged-engine verification.

## Installation

Download the latest desktop build from [GitHub Releases](https://github.com/nimbold/Firelink/releases).

| Platform | Package | Notes |
| --- | --- | --- |
| **macOS Apple silicon** | `.dmg` | Unsigned and not notarized. Open through Finder or approve once in **System Settings -> Privacy & Security**. |
| **Windows x64** | NSIS `.exe` installer | Unsigned. Windows SmartScreen may warn until code signing is added. |
| **Linux x64** | `.AppImage` | Make executable before launching if your desktop environment does not do that automatically. |

Production bundles include the required media engines for the target platform. Users do not need to install aria2, yt-dlp, FFmpeg, Deno, Python, Homebrew, or a package manager for normal app usage.

## Browser Extension

<div align="center">
  <a href="https://addons.mozilla.org/en-US/firefox/addon/firelink-companion/"><img src="https://img.shields.io/badge/Install%20Firelink%20Companion-Firefox-FF7139?style=for-the-badge&logo=firefox-browser&logoColor=white" alt="Install Firelink Companion on Firefox" /></a>
</div>

Firelink Companion connects your browser to the desktop app so links and browser downloads can open in Firelink instead of disappearing into the browser's default download shelf.

What it adds:

- **Automatic capture** for normal browser downloads, while still routing every captured link through Firelink's Add window.
- **Context-menu actions** for "Download with Firelink" and selected links.
- **Signed local requests** using the pairing token from **Settings -> Integrations**.
- **Server identity checks** so the extension only trusts the real local Firelink app.
- **Offline-safe behavior** that resumes browser downloads when Firelink is closed or rejects a handoff.
- **Protocol-aware compatibility** so older desktop builds are rejected before automatic capture can cancel a browser download.

Install the extension, open Firelink, then pair it from **Settings -> Integrations**. The Firefox add-on is maintained in the [Firelink-Extension](https://github.com/nimbold/Firelink-Extension) repository and is also vendored here as the `Extensions/Firefox` submodule.

## Platforms

| Target | Status |
| --- | --- |
| **macOS arm64** | Supported. Automated native build, engine validation, packaged launch smoke test, and unsigned DMG packaging. |
| **Windows x64** | Supported. Native GitHub Actions build, engine validation, silent installer smoke test, and NSIS packaging. |
| **Linux x64** | Supported. Native GitHub Actions build, engine validation, AppImage repackaging, and xvfb launch smoke test. |

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

macOS development uses locked payloads in `src-tauri/binaries`. Windows and Linux payloads are provisioned from checksum-pinned archives:

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
└── Extensions/Firefox/   Firelink Companion submodule
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
