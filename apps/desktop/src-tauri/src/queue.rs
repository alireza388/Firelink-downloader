use tokio::sync::mpsc;
use tauri::Manager;

#[derive(Clone)]
pub struct DownloadTask {
    pub is_media: bool,
    pub id: String,
    pub url: String,
    pub destination: String,
    pub filename: String,
    pub connections: Option<i32>,
    pub speed_limit: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers: Option<String>,
    pub checksum: Option<String>,
    pub cookies: Option<String>,
    pub mirrors: Option<String>,
    pub user_agent: Option<String>,
    pub max_tries: Option<i32>,
    pub proxy: Option<String>,
    pub format_selector: Option<String>,
    pub cookie_source: Option<String>,
}

pub enum DownloadCommand {
    Enqueue(DownloadTask),
    SetLimit(usize),
    TaskFinished(String), // id
}

pub fn setup_queue(app_handle: tauri::AppHandle, mut cmd_rx: mpsc::UnboundedReceiver<DownloadCommand>) {
    tauri::async_runtime::spawn(async move {
        let mut queue = std::collections::VecDeque::new();
        let mut active_count = 0;
        let mut limit = 3;

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                DownloadCommand::Enqueue(task) => {
                    queue.push_back(task);
                }
                DownloadCommand::SetLimit(l) => {
                    limit = l;
                }
                DownloadCommand::TaskFinished(_) => {
                    if active_count > 0 {
                        active_count -= 1;
                    }
                }
            }

            while active_count < limit && !queue.is_empty() {
                if let Some(task) = queue.pop_front() {
                    active_count += 1;
                    let ah = app_handle.clone();
                    let state = ah.state::<crate::AppState>();
                    let tx = state.cmd_tx.clone();
                    let tasks_map = state.tasks.clone();
                    
                    if task.is_media {
                        let _ = crate::start_media_download_internal(ah, tasks_map, tx, task).await;
                    } else {
                        let _ = crate::start_download_internal(ah, tasks_map, tx, task).await;
                    }
                }
            }
        }
    });
}
