# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.2] - 2026-06-03

### Features added
- Added double-click to open completed files directly from the download table.
- Added redownload functionality for completed or failed items.
- Added 'Copy Address' context menu action.
- Added a monochrome template tray icon loaded explicitly with precise dimensions.

### Changes
- Improved context menu organization and conditionally displayed actions based on download status.

## [0.4.1] - 2026-06-03

### Features added
- Added app theming engine with Look and Feel settings.
- Added Font Size, List Row Density, and Menu Bar Icon settings.
- Added tray icon and context menu for main window and queues.
- Added site logins integration directly into the Add Downloads window.

### Changes
- Updated the paste hint to use a visual Command icon.

### Fixes
- Resolved SwiftUI infinite layout freeze caused by MenuBarExtra binding.
- Fixed a bug with Light/System theme appearance.
- Fixed phantom state issues with Menu Bar Icon setting and conditionally applied theme backgrounds to preserve native macOS translucency.

## [0.4.0] - 2026-06-03

### Changes
- Reorganized Settings sections so related download preferences sit together and app diagnostics live under App.
- Hardened the release workflow with explicit macOS 26 SDK checks, newer GitHub Actions, and app signature verification.
- Prefer the bundled `aria2c` binary inside release builds.

### Fixes
- Fixed queue-specific starts so one queue no longer starts unrelated queued downloads.
- Fixed scheduler completion handling so empty queues do not trigger post-download system actions.
- Fixed queue drag reordering when moving items downward.
- Fixed scheduler Automation permission prompting.

### Features added
- Added scheduler controls with explicit Automation permission UI.
- Added global and per-download speed limits.
- Added advanced transfer options for checksums, headers, cookies, and mirror URLs.

## [0.3.0] - 2026-06-02

### Added
- **Zero-Config Setup:** Firelink now automatically bundles the `aria2c` engine and all of its dynamic library dependencies internally via `dylibbundler`. End-users no longer need to install Homebrew or `aria2c` manually! 

### Changed
- **README Redesign:** Modernized the README with a clean layout, centered App Icon header, and updated roadmap.
- **CI Releases:** The GitHub Actions DMG release pipeline now automatically fetches and packages dependencies during builds.

## [0.2.1] - 2026-06-02
### Changed
- Fixed CI release runner specifying macOS 26.

## [0.2.0] - 2026-06-01
### Added
- **In-App Update Checker:** Built-in GitHub release checks inside the Settings About pane.
- **Queue Management:** Advanced drag-and-drop priority ordering and queue management controls.
- **Download Recovery:** Built-in download recovery and automated retry policies.
- Initial core download engine with `aria2c` support.
- Native macOS Settings pane.
- Smart file categorization and organization based on extension detection.
- Keychain-secured authentication integration.
