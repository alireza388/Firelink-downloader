use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};

#[derive(Debug)]
pub enum DownloadCmd {
    CaptureUrls(Vec<String>),
    FrontendReady(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadEvent {
    CapturedUrls(String),
}

#[derive(Clone)]
pub struct DownloadCoordinator {
    tx: mpsc::Sender<DownloadCmd>,
    media_tx: mpsc::Sender<MediaCmd>,
}

impl DownloadCoordinator {
    pub fn spawn(app_handle: AppHandle) -> Self {
        Self::spawn_with_events(CoordinatorEventSink::Tauri(app_handle))
    }

    pub fn spawn_headless() -> (Self, mpsc::UnboundedReceiver<DownloadEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        (
            Self::spawn_with_events(CoordinatorEventSink::Headless(event_tx)),
            event_rx,
        )
    }

    fn spawn_with_events(events: CoordinatorEventSink) -> Self {
        let (tx, rx) = mpsc::channel(128);
        let (media_tx, media_rx) = mpsc::channel(32);
        tauri::async_runtime::spawn(run_coordinator(events, rx, media_rx));
        Self { tx, media_tx }
    }

    pub async fn send(&self, command: DownloadCmd) -> Result<(), String> {
        self.tx
            .send(command)
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())
    }

    pub async fn register_media(
        &self,
        id: String,
        lifecycle_generation: u64,
    ) -> Result<watch::Receiver<bool>, String> {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.media_tx
            .send(MediaCmd::Register {
                id,
                lifecycle_generation,
                cancel_tx,
            })
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())?;
        Ok(cancel_rx)
    }

    pub async fn pause_media(&self, id: String, lifecycle_generation: u64) -> Result<(), String> {
        self.media_tx
            .send(MediaCmd::Pause {
                id,
                lifecycle_generation,
            })
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())
    }

    pub async fn pause_media_with_ack(
        &self,
        id: String,
        lifecycle_generation: u64,
        ack: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(), String> {
        self.media_tx
            .send(MediaCmd::PauseWithAck {
                id,
                lifecycle_generation,
                ack,
            })
            .await
            .map_err(|_| "download coordinator is unavailable".to_string())
    }

    pub async fn finish_media(&self, id: String, lifecycle_generation: u64) {
        let _ = self
            .media_tx
            .send(MediaCmd::Finished {
                id,
                lifecycle_generation,
            })
            .await;
    }
}

#[derive(Clone)]
enum CoordinatorEventSink {
    Tauri(AppHandle),
    Headless(mpsc::UnboundedSender<DownloadEvent>),
}

impl CoordinatorEventSink {
    fn emit_captured_urls(&self, payload: String) -> bool {
        match self {
            Self::Tauri(app_handle) => app_handle.emit("deep-link-add-download", payload).is_ok(),
            Self::Headless(event_tx) => event_tx.send(DownloadEvent::CapturedUrls(payload)).is_ok(),
        }
    }
}

enum MediaCmd {
    Register {
        id: String,
        lifecycle_generation: u64,
        cancel_tx: watch::Sender<bool>,
    },
    Pause {
        id: String,
        lifecycle_generation: u64,
    },
    PauseWithAck {
        id: String,
        lifecycle_generation: u64,
        ack: tokio::sync::oneshot::Sender<()>,
    },
    Finished {
        id: String,
        lifecycle_generation: u64,
    },
}

async fn run_coordinator(
    events: CoordinatorEventSink,
    mut command_rx: mpsc::Receiver<DownloadCmd>,
    mut media_rx: mpsc::Receiver<MediaCmd>,
) {
    let mut active_media = HashMap::<String, (u64, watch::Sender<bool>)>::new();
    let mut pending_media_acks = HashMap::<String, (u64, tokio::sync::oneshot::Sender<()>)>::new();
    let mut pending_captured_urls = Vec::<String>::new();
    let mut frontend_ready = false;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    break;
                };

                match command {
                    DownloadCmd::CaptureUrls(urls) => {
                        append_unique_urls(&mut pending_captured_urls, urls);
                        if frontend_ready && !pending_captured_urls.is_empty() {
                            let payload = pending_captured_urls.join("\n");
                            if events.emit_captured_urls(payload) {
                                pending_captured_urls.clear();
                            }
                        }
                    }
                    DownloadCmd::FrontendReady(ready) => {
                        frontend_ready = ready;
                        if ready && !pending_captured_urls.is_empty() {
                            let payload = pending_captured_urls.join("\n");
                            if events.emit_captured_urls(payload) {
                                pending_captured_urls.clear();
                            }
                        }
                    }
                }
            }
            command = media_rx.recv() => {
                let Some(command) = command else {
                    continue;
                };
                match command {
                    MediaCmd::Register { id, lifecycle_generation, cancel_tx } => {
                        if let Some((_, previous)) = active_media.insert(id, (lifecycle_generation, cancel_tx)) {
                            let _ = previous.send(true);
                        }
                    }
                    MediaCmd::Pause { id, lifecycle_generation } => {
                        if active_media.get(&id).is_some_and(|(generation, _)| *generation == lifecycle_generation) {
                            if let Some((_, cancel_tx)) = active_media.remove(&id) {
                                let _ = cancel_tx.send(true);
                            }
                        }
                    }
                    MediaCmd::PauseWithAck { id, lifecycle_generation, ack } => {
                        if active_media.get(&id).is_some_and(|(generation, _)| *generation == lifecycle_generation) {
                            if let Some((_, cancel_tx)) = active_media.remove(&id) {
                                let _ = cancel_tx.send(true);
                                pending_media_acks.insert(id, (lifecycle_generation, ack));
                            }
                        } else {
                            let _ = ack.send(());
                        }
                    }
                    MediaCmd::Finished { id, lifecycle_generation } => {
                        if active_media.get(&id).is_some_and(|(generation, _)| *generation == lifecycle_generation) {
                            active_media.remove(&id);
                        }
                        if pending_media_acks.get(&id).is_some_and(|(generation, _)| *generation == lifecycle_generation) {
                            if let Some((_, ack)) = pending_media_acks.remove(&id) {
                                let _ = ack.send(());
                            }
                        }
                    }
                }
            }
        }
    }

    for (_, (_, cancel_tx)) in active_media {
        let _ = cancel_tx.send(true);
    }
}

fn append_unique_urls(target: &mut Vec<String>, urls: Vec<String>) {
    let mut seen = target.iter().cloned().collect::<HashSet<_>>();
    target.extend(urls.into_iter().filter(|url| seen.insert(url.clone())));
}

pub(crate) fn format_speed(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_second / (1024.0 * 1024.0))
    } else if bytes_per_second >= 1024.0 {
        format!("{:.1} KB/s", bytes_per_second / 1024.0)
    } else {
        format!("{bytes_per_second:.0} B/s")
    }
}

pub(crate) fn format_size(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB", bytes / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024.0 * 1024.0 {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}

pub(crate) fn format_duration(seconds: f64) -> String {
    if seconds >= 3600.0 {
        format!("{:.0}h {:.0}m", seconds / 3600.0, (seconds % 3600.0) / 60.0)
    } else if seconds >= 60.0 {
        format!("{:.0}m {:.0}s", seconds / 60.0, seconds % 60.0)
    } else {
        format!("{seconds:.0}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadCmd, DownloadCoordinator, DownloadEvent};
    use std::time::Duration;

    #[tokio::test]
    async fn buffers_captured_urls_until_frontend_is_ready() {
        let (coordinator, mut events) = DownloadCoordinator::spawn_headless();
        coordinator
            .send(DownloadCmd::CaptureUrls(vec![
                "https://example.com/startup.zip".to_string(),
            ]))
            .await
            .unwrap();

        assert!(
            tokio::time::timeout(Duration::from_millis(20), events.recv())
                .await
                .is_err()
        );

        coordinator
            .send(DownloadCmd::FrontendReady(true))
            .await
            .unwrap();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap(),
            DownloadEvent::CapturedUrls("https://example.com/startup.zip".to_string())
        );
    }

    #[tokio::test]
    async fn stale_media_finish_cannot_remove_a_newer_lifecycle() {
        let (coordinator, _events) = DownloadCoordinator::spawn_headless();
        let mut old_cancel = coordinator.register_media("same-id".to_string(), 1).await.unwrap();
        let mut new_cancel = coordinator.register_media("same-id".to_string(), 2).await.unwrap();

        tokio::time::timeout(Duration::from_secs(1), old_cancel.changed())
            .await
            .unwrap()
            .unwrap();
        coordinator.finish_media("same-id".to_string(), 1).await;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        coordinator
            .pause_media_with_ack("same-id".to_string(), 2, ack_tx)
            .await
            .unwrap();
        coordinator.finish_media("same-id".to_string(), 2).await;
        tokio::time::timeout(Duration::from_secs(1), ack_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(*new_cancel.borrow_and_update());
    }
}
