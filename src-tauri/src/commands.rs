use std::process::Command;
use std::path::Path;

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }
    
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer.exe")
            .arg(format!("/select,\"{}\"", path))
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let parent = Path::new(&path).parent().unwrap_or(Path::new(""));
        Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| format!("Failed to reveal file: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
pub async fn open_downloaded_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }
    
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn trash_download_assets(path: String, partial_paths: Vec<String>) -> Result<(), String> {
    let p = Path::new(&path);
    if p.exists() {
        if let Err(e) = trash::delete(p) {
            return Err(format!("Failed to trash primary file: {}", e));
        }
    }

    for partial in partial_paths {
        let p_partial = Path::new(&partial);
        if p_partial.exists() {
            if let Err(e) = trash::delete(p_partial) {
                return Err(format!("Failed to trash partial file: {}", e));
            }
        }
    }

    Ok(())
}
