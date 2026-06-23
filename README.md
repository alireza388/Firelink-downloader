<div align="center">
  <img src="src/assets/app-icon.png" alt="Firelink" width="112" height="112" />

  # Firelink

  **A fast, focused desktop download manager powered by Rust and Tauri.**

  [![Version](https://img.shields.io/badge/version-0.7.3-6f42c1?style=flat-square)](https://github.com/nimbold/Firelink/releases)
  [![Platform](https://img.shields.io/badge/platform-macOS-111111?style=flat-square&logo=apple)](#project-status)
  [![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
  [![Rust](https://img.shields.io/badge/Rust-backend-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)
  [![License](https://img.shields.io/github/license/nimbold/Firelink?style=flat-square)](LICENSE)
  [![CI](https://img.shields.io/github/actions/workflow/status/nimbold/Firelink/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/nimbold/Firelink/actions/workflows/ci.yml)

  [Features](#features) · [Install](#installation) · [Development](#development) · [Project status](#project-status)
</div>

Firelink brings segmented downloads, media extraction, scheduling, and browser
integration into one native-feeling desktop application. The current app uses a
Rust backend with a React and TypeScript interface, replacing the original
SwiftUI implementation.

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
> The Rust and Tauri application is still completing release packaging. The
> current `v0.7.3` macOS asset on GitHub Releases belongs to the archived SwiftUI
> implementation.

For now, run the maintained application from source using the development
instructions below. New packaged builds will be published through
[GitHub Releases](https://github.com/nimbold/Firelink/releases) once the
migration is complete.

Production bundles include target-specific media engines, so packaged releases
do not require separate aria2, yt-dlp, FFmpeg, Deno, Python, or package-manager
installations.

macOS builds are distributed without Apple code signing or notarization. Users
must approve the downloaded app through Finder or **System Settings → Privacy &
Security**. Firelink does not claim Gatekeeper trust.

### Browser Extension

Install [Firelink Companion for Firefox](https://addons.mozilla.org/en-US/firefox/addon/firelink-companion/)
to send browser downloads directly to Firelink. Pair the extension from
**Settings → Integrations** using the generated local token.

## Project Status

The migration from SwiftUI to Rust, Tauri, React, and TypeScript is in its final
stage. The new application is the maintained implementation at the repository
root.

| Target | Status |
| --- | --- |
| macOS arm64 | Automated build, engine validation, and unsigned DMG packaging |
| Windows x64 | Native CI and NSIS packaging configured; first clean-run validation pending |
| Linux x64 | Native CI and AppImage packaging configured; desktop-matrix validation pending |

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

macOS development uses locked payloads in `src-tauri/binaries`. Windows and
Linux payloads are provisioned from checksum-pinned archives:

```sh
node scripts/provision-engines.js --target x86_64-pc-windows-msvc
node scripts/provision-engines.js --target x86_64-unknown-linux-gnu
```

Build staging includes only current target. See `engines.lock.json`,
`engine-sources.lock.json`, and [RELEASE.md](RELEASE.md).

## Repository Structure

```text
.
├── src/                  React and TypeScript interface
├── src-tauri/            Rust backend and Tauri configuration
├── Extensions/Firefox/   Firelink Companion submodule
└── legacy/swift/         Archived SwiftUI application
```

## Technology

- [Tauri 2](https://tauri.app/) for the desktop runtime
- [Rust](https://www.rust-lang.org/) and [Tokio](https://tokio.rs/) for native application logic
- [React](https://react.dev/) and [TypeScript](https://www.typescriptlang.org/) for the interface
- [Zustand](https://zustand-demo.pmnd.rs/) for frontend state
- [SQLite](https://www.sqlite.org/) for persistent application data
- [aria2](https://aria2.github.io/), [yt-dlp](https://github.com/yt-dlp/yt-dlp),
  [FFmpeg](https://ffmpeg.org/), and [Deno](https://deno.com/) for download and media processing

## License

Firelink is available under the [MIT License](LICENSE).
