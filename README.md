# Firelink

Firelink is a clean SwiftUI download manager for Apple Silicon Macs. The goal is to bring the practical parts of IDM/FDM-style download management to macOS with a native interface, segmented downloads, queue control, automatic file organization, and credential-aware transfers.

This project is early, but it already has a working native prototype and an `aria2c`-backed download engine.

## Features

- Native SwiftUI macOS interface.
- Segmented downloads with 16-32 requested parts per file.
- Multiple files downloading at the same time.
- Queue-based downloads with drag-and-drop priority ordering.
- Native macOS Settings window, available from App menu > Settings and the main toolbar.
- Configurable per-server connection count.
- Automatic save folders under `~/Downloads`:
  - `Musics`
  - `Movies`
  - `Compressed`
  - `Pictures`
  - `Documents`
  - `Other`
- Custom download locations per file category.
- Broad file extension detection for audio, video, archive, image, and document formats.
- HTTP, HTTPS, FTP, and SFTP URL support through `aria2c`.
- Site login rules with URL pattern matching and Keychain-stored passwords.
- Optional prevention of system sleep while files are downloading, while still allowing display sleep.
- Pause, resume, cancel, delete, progress, speed, ETA, and connection count display.
- Release `.app` bundle script for local macOS builds.

## Engine

This first version uses `aria2c` as the download engine. It is a better fit than plain `curl` for the requested IDM/FDM-style behavior because it has segmented downloads, resumable transfers, concurrent downloads, HTTP/FTP/SFTP support, and username/password options built in.

The UI allows 16-32 requested parts. For ordinary same-host HTTP downloads, Firelink currently caps `aria2c`'s per-server connection count at 16 while still setting the requested split count. This keeps behavior aligned with common server limits and `aria2c`'s stable controls.

## Requirements

Install the engine:

```sh
brew install aria2
```

- macOS 14 or newer.
- Apple Silicon Mac.
- Swift 6 toolchain.
- `aria2c` installed with Homebrew, or bundled into the app resources later.

## Run

```sh
swift run Firelink
```

Build a release `.app` bundle:

```sh
make app
open build/Firelink.app
```

Because the current machine only has Command Line Tools selected, this repository is set up as a Swift Package with a bundling script rather than a generated Xcode project. Opening the package in Xcode will still give you a native macOS app workflow.

## Roadmap

- Persist download history and queue state.
- Improve site-login editing and migration tools.
- Add browser integration and URL capture.
- Add scheduler rules and speed limits.
- Add checksum verification.
- Add richer failure recovery and retry policies.
- Add unit tests for file classification, queue behavior, and `aria2c` progress parsing.

## License

Firelink is released under the MIT License. See [LICENSE](LICENSE).
