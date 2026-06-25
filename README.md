<div align="center">
  <img src="src/assets/app-icon.png" alt="Firelink" width="112" height="112" />

  # Firelink

  **A fast, focused desktop download manager powered by Rust and Tauri.**

  [![Version](https://img.shields.io/badge/version-0.7.3-6f42c1?style=flat-square)](https://github.com/nimbold/Firelink/releases)
  [![macOS](https://img.shields.io/badge/macOS-111111?style=flat-square&logo=apple&logoColor=white)](#project-status)
  [![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=microsoft&logoColor=white)](#project-status)
  [![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)](#project-status)
  [![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
  [![Rust](https://img.shields.io/badge/Rust-backend-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)
  [![License](https://img.shields.io/github/license/nimbold/Firelink?style=flat-square)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/nimbold/Firelink/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/nimbold/Firelink/actions/workflows/ci.yml)

  [Features](#features) · [Install](#installation) · [Development](#development) · [Project status](#project-status)
</div>

<br/>

<div align="center">
  <img src="Screenshots/Dark%20theme%20-%20main.png" width="24%" alt="Dark Theme Main Window" />
  <img src="Screenshots/Dark%20theme%20-%20add%20window.png" width="24%" alt="Dark Theme Add Window" />
  <img src="Screenshots/Light%20theme%20-%20main.png" width="24%" alt="Light Theme Main Window" />
  <img src="Screenshots/Light%20theme%20-%20add%20window.png" width="24%" alt="Light Theme Add Window" />

  <details>
    <summary><b>View More Screenshots</b></summary>
    <br/>
    <img src="Screenshots/Dark%20theme%20-%20settings.png" width="24%" alt="Dark Theme Settings" />
    <img src="Screenshots/Light%20theme%20-%20settings.png" width="24%" alt="Light Theme Settings" />
  </details>
</div>

<br/>

Firelink brings segmented downloads, media extraction, scheduling, and browser integration into one native-feeling desktop application. The application is built with a Rust backend and a React/TypeScript interface, providing excellent cross-platform support.

## Features

- **Fast transfers** with segmented downloading powered by aria2
- **Media downloads** through yt-dlp, FFmpeg, and Deno
- **Persistent queues** with configurable concurrency and speed limits
- **Download scheduling** with optional post-queue system actions
- **Pause, resume, retry, and duplicate-file handling**
- **Smart organization** with categories and configurable destinations
- **Browser integration** through the Firelink Companion extension
- **Secure local handoff** using authenticated extension pairing
- **System integration** including tray controls, notifications, and sleep prevention
- **Built-in update checks** backed by GitHub Releases

## Installation

> [!IMPORTANT]
> The cross-platform rust implementation is completing release packaging for macOS, Windows, and Linux. The current `v0.7.3` macOS asset on GitHub Releases belongs to the archived SwiftUI implementation.

For now, run the maintained application from source using the development instructions below. New packaged builds will be published through [GitHub Releases](https://github.com/nimbold/Firelink/releases) once the packaging CI workflow is fully ready.

Production bundles include target-specific media engines, so packaged releases do not require separate aria2, yt-dlp, FFmpeg, Deno, Python, or package-manager installations.

macOS builds are distributed without Apple code signing or notarization. Users must approve the downloaded app through Finder or **System Settings → Privacy & Security**. Firelink does not claim Gatekeeper trust.

## 🧩 Browser Extension

[![Install on Firefox](https://img.shields.io/badge/Install%20on-Firefox-FF7139?style=flat-square&logo=firefox&logoColor=white)](https://addons.mozilla.org/en-US/firefox/addon/firelink-companion/)

Install the companion extension to send browser downloads directly to Firelink. Pair the extension from **Settings → Integrations** using the generated local token.

## Project Status

The cross-platform audit has been successfully completed. Firelink implements robust OS-specific behaviors to ensure native integration and stability across all platforms. 

| Target | Status |
| --- | --- |
| **macOS arm64** | Fully supported. Automated build, engine validation, and unsigned DMG packaging complete. |
| **Windows x64** | Fully supported. Native CI and NSIS packaging configured. |
| **Linux x64** | Fully supported. Native CI and AppImage packaging configured. |

See the [changelog](CHANGELOG.md) for release history and recent work.

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

Run the frontend build and backend tests:

```sh
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
├── src-tauri/            Rust backend and Tauri configuration
└── Extensions/Firefox/   Firelink Companion submodule
```

## Technology & Credits

Firelink is made possible by these incredible open-source projects:

- **[Tauri 2](https://tauri.app/)** for the lightweight, secure desktop runtime
- **[Rust](https://www.rust-lang.org/)** and **[Tokio](https://tokio.rs/)** for high-performance native application logic
- **[React](https://react.dev/)** and **[TypeScript](https://www.typescriptlang.org/)** for the responsive user interface
- **[Zustand](https://zustand-demo.pmnd.rs/)** for simplified frontend state management
- **[SQLite](https://www.sqlite.org/)** for reliable, persistent application data
- **[aria2](https://aria2.github.io/)** for blazing fast segmented downloading
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**, **[FFmpeg](https://ffmpeg.org/)**, and **[Deno](https://deno.com/)** for unmatched media extraction and processing capabilities

## License

Firelink is available under the [MIT License](LICENSE).
