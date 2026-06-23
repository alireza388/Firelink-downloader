use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ipc::DownloadCategory;

#[tauri::command]
pub async fn get_system_proxy() -> Result<Option<String>, String> {
    match sysproxy::Sysproxy::get_system_proxy() {
        Ok(proxy) => {
            if proxy.enable {
                let protocol = if proxy.host.contains("://") { "" } else { "http://" };
                Ok(Some(format!("{}{}:{}", protocol, proxy.host, proxy.port)))
            } else {
                Ok(None)
            }
        }
        Err(error) => Err(format!("failed to read system proxy settings: {error}")),
    }
}

#[tauri::command]
pub fn get_file_category(filename: String) -> DownloadCategory {
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let music_exts = ["mp3", "wav", "aac", "flac", "ogg", "m4a", "wma", "alac", "ape", "mid", "midi"];
    let movie_exts = ["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpeg", "mpg", "3gp", "ts", "vob"];
    let compressed_exts = ["zip", "rar", "7z", "tar", "gz", "xz", "bz2", "lz", "lzma", "zst", "iso", "cab", "tgz", "tbz", "z", "sit", "sitx"];
    let picture_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff", "svg", "ico", "heic", "raw", "psd", "ai"];
    let document_exts = ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "csv", "md", "epub", "mobi", "azw3"];
    let app_exts = ["exe", "msi", "bat", "cmd", "app", "dmg", "pkg", "apk", "appx", "deb", "rpm", "appimage", "run", "sh", "bin", "jar"];

    if music_exts.contains(&ext.as_str()) {
        DownloadCategory::Musics
    } else if movie_exts.contains(&ext.as_str()) {
        DownloadCategory::Movies
    } else if compressed_exts.contains(&ext.as_str()) {
        DownloadCategory::Compressed
    } else if picture_exts.contains(&ext.as_str()) {
        DownloadCategory::Pictures
    } else if document_exts.contains(&ext.as_str()) {
        DownloadCategory::Documents
    } else if app_exts.contains(&ext.as_str()) {
        DownloadCategory::Applications
    } else {
        DownloadCategory::Other
    }
}

#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AvailableReleaseUpdate {
    pub version: String,
    pub tag_name: String,
    pub title: String,
    pub release_notes: String,
    pub release_url: String,
    pub published_at: Option<String>,
}

#[derive(Serialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ReleaseCheckOutcome {
    UpdateAvailable { update: AvailableReleaseUpdate },
    UpToDate { latest_version: String, local_version: String },
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates(app_handle: tauri::AppHandle) -> Result<ReleaseCheckOutcome, String> {
    let current_version = app_handle.package_info().version.to_string();
    
    let client = reqwest::Client::new();
    let res = client.get("https://api.github.com/repos/nimbold/Firelink/releases?per_page=30")
        .header("User-Agent", "Firelink")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("GitHub returned HTTP {}", res.status().as_u16()));
    }

    let releases: Vec<GitHubRelease> = res.json().await.map_err(|e| e.to_string())?;
    
    let latest_stable = releases.into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .max_by(|a, b| cmp_versions(&a.tag_name, &b.tag_name));
        
    let release = match latest_stable {
        Some(r) => r,
        None => return Err("No stable release was found.".to_string()),
    };

    let latest_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();

    if cmp_versions(&latest_version, &current_version) == std::cmp::Ordering::Greater {
        Ok(ReleaseCheckOutcome::UpdateAvailable {
            update: AvailableReleaseUpdate {
                version: latest_version.clone(),
                tag_name: release.tag_name.clone(),
                title: release.name.unwrap_or(release.tag_name),
                release_notes: release.body.unwrap_or_else(|| "No release notes were provided for this version.".to_string()),
                release_url: release.html_url,
                published_at: release.published_at,
            }
        })
    } else {
        Ok(ReleaseCheckOutcome::UpToDate {
            latest_version,
            local_version: current_version,
        })
    }
}

fn cmp_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use semver::Version;
    
    let a_clean = a.trim_start_matches(['v', 'V']);
    let b_clean = b.trim_start_matches(['v', 'V']);
    
    let a_ver = Version::parse(a_clean).unwrap_or_else(|_| Version::new(0, 0, 0));
    let b_ver = Version::parse(b_clean).unwrap_or_else(|_| Version::new(0, 0, 0));
    
    a_ver.cmp(&b_ver)
}

#[tauri::command]
pub async fn create_category_directories(
    app_handle: tauri::AppHandle,
    base_folder: String,
    subfolders: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let base = crate::resolve_path(&base_folder, &app_handle);
    let mut errors = Vec::new();

    for subfolder in subfolders.values() {
        let normalized = subfolder.replace('\\', "/");
        let relative = std::path::Path::new(&normalized);
        if relative.is_absolute()
            || normalized
                .split('/')
                .any(|part| part == ".." || part.ends_with(':'))
        {
            return Err(format!(
                "Category subfolder must be a relative path: {subfolder}"
            ));
        }

        let expanded = base.join(relative);
        if !expanded.exists() {
            if let Err(e) = tokio::fs::create_dir_all(&expanded).await {
                errors.push(format!("{}: {}", expanded.display(), e));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to create one or more category directories ({})",
            errors.join("; ")
        ))
    }
}

pub static SUPPORTED_DOMAINS: &[&str] = &[
    "youtube.com", "youtu.be",
    "twitter.com", "x.com",
    "vimeo.com",
    "twitch.tv",
    "instagram.com",
    "tiktok.com",
    "facebook.com", "fb.watch",
    "reddit.com", "v.redd.it",
    "soundcloud.com",
];

#[tauri::command]
pub fn get_supported_media_domains() -> Vec<String> {
    SUPPORTED_DOMAINS.iter().map(|s| s.to_string()).collect()
}

#[tauri::command]
pub fn is_supported_media(url: String) -> bool {
    if let Ok(parsed_url) = reqwest::Url::parse(&url) {
        if let Some(host) = parsed_url.host_str() {
            let host_lower = host.to_lowercase();
            for domain in SUPPORTED_DOMAINS.iter() {
                if host_lower == *domain || host_lower.ends_with(&format!(".{}", domain)) {
                    return true;
                }
            }
        }
    }
    false
}
