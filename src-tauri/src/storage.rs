use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

pub const PORTABLE_MARKER: &str = "portable.flag";
const PORTABLE_DATA_DIR: &str = "data";
const PORTABLE_LOG_DIR: &str = "logs";
const PORTABLE_WEBVIEW_DIR: &str = "webview";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageMode {
    Standard,
    Portable { root: PathBuf },
}

impl StorageMode {
    pub fn detect() -> Self {
        let Some(executable) = std::env::current_exe().ok() else {
            return Self::Standard;
        };
        let Some(root) = executable.parent() else {
            return Self::Standard;
        };

        if root.join(PORTABLE_MARKER).is_file() {
            Self::Portable {
                root: root.to_path_buf(),
            }
        } else {
            Self::Standard
        }
    }

    #[cfg(test)]
    fn detect_from_root(root: &Path) -> Self {
        if root.join(PORTABLE_MARKER).is_file() {
            Self::Portable {
                root: root.to_path_buf(),
            }
        } else {
            Self::Standard
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    mode: StorageMode,
    data_dir: PathBuf,
    log_dir: PathBuf,
    webview_dir: PathBuf,
}

impl StorageLayout {
    pub fn resolve<R: Runtime>(
        app_handle: &AppHandle<R>,
        mode: StorageMode,
    ) -> Result<Self, String> {
        match mode {
            StorageMode::Standard => Ok(Self {
                mode: StorageMode::Standard,
                data_dir: app_handle
                    .path()
                    .app_data_dir()
                    .map_err(|error| format!("failed to resolve app data directory: {error}"))?,
                log_dir: app_handle
                    .path()
                    .app_log_dir()
                    .map_err(|error| format!("failed to resolve app log directory: {error}"))?,
                webview_dir: app_handle.path().app_local_data_dir().map_err(|error| {
                    format!("failed to resolve app local data directory: {error}")
                })?,
            }),
            StorageMode::Portable { root } => {
                let data_dir = root.join(PORTABLE_DATA_DIR);
                Ok(Self {
                    mode: StorageMode::Portable { root },
                    log_dir: data_dir.join(PORTABLE_LOG_DIR),
                    webview_dir: data_dir.join(PORTABLE_WEBVIEW_DIR),
                    data_dir,
                })
            }
        }
    }

    pub fn is_portable(&self) -> bool {
        matches!(self.mode, StorageMode::Portable { .. })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn webview_dir(&self) -> &Path {
        &self.webview_dir
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageMode, PORTABLE_MARKER};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn marker_selects_portable_mode() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join(PORTABLE_MARKER), b"portable\n").unwrap();

        assert_eq!(
            StorageMode::detect_from_root(root.path()),
            StorageMode::Portable {
                root: root.path().to_path_buf()
            }
        );
    }

    #[test]
    fn missing_marker_keeps_standard_mode() {
        let root = TempDir::new().unwrap();

        assert_eq!(
            StorageMode::detect_from_root(root.path()),
            StorageMode::Standard
        );
    }
}
