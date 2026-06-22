import { initMediaDomains, isActiveDownloadStatus, normalizeSpeedLimitForBackend } from './utils/downloads';
import { useEffect, useRef, useState } from "react";
import { Sidebar, SidebarFilter } from "./components/Sidebar";
import { DownloadTable } from "./components/DownloadTable";
import { AddDownloadsModal } from "./components/AddDownloadsModal";
import SettingsView from "./components/SettingsView";
import { PropertiesModal } from "./components/PropertiesModal";
import { DeleteConfirmationModal } from "./components/DeleteConfirmationModal";
import { extractValidDownloadUrls } from './utils/url';
import { listenEvent as listen, invokeCommand as invoke } from "./ipc";
import { useDownloadStore, MAIN_QUEUE_ID } from './store/useDownloadStore';
import { initDownloadListener } from './store/downloadStore';
import { useSettingsStore } from "./store/useSettingsStore";
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import SchedulerView from "./components/SchedulerView";
import SpeedLimiterView from "./components/SpeedLimiterView";
import LogsView from "./components/LogsView";
import { useToast } from "./contexts/ToastContext";
import { openUrl } from '@tauri-apps/plugin-opener';

let automaticUpdateCheckStarted = false;
const processingScheduleKeys = new Set<string>();

const getScheduledQueueIds = () => {
  const downloadState = useDownloadStore.getState();
  const availableQueueIds = new Set(downloadState.queues.map(queue => queue.id));
  const selectedQueueIds = useSettingsStore.getState().scheduler.selectedQueueIds
    .filter(queueId => availableQueueIds.has(queueId));
  return selectedQueueIds.length > 0 ? selectedQueueIds : [MAIN_QUEUE_ID];
};

function App() {
  const [filter, setFilter] = useState<SidebarFilter>('all');

  const [sidebarWidth, setSidebarWidth] = useState(() => {
    const stored = Number(window.localStorage.getItem('firelink-sidebar-width'));
    return Number.isFinite(stored) && stored >= 190 && stored <= 260 ? stored : 220;
  });

  const theme = useSettingsStore(state => state.theme);
  const isSidebarVisible = useSettingsStore(state => state.isSidebarVisible);
  const activeView = useSettingsStore(state => state.activeView);
  const appFontSize = useSettingsStore(state => state.appFontSize);
  const listRowDensity = useSettingsStore(state => state.listRowDensity);
  const autoCheckUpdates = useSettingsStore(state => state.autoCheckUpdates);
  const showNotifications = useSettingsStore(state => state.showNotifications);
  const showDockBadge = useSettingsStore(state => state.showDockBadge);
  const showMenuBarIcon = useSettingsStore(state => state.showMenuBarIcon);
  const extensionPairingToken = useSettingsStore(state => state.extensionPairingToken);
  const downloads = useDownloadStore(state => state.downloads);
  const activeDownloadCount = downloads.filter(download => download.status === 'downloading').length;
  const queuedCount = downloads.filter(download =>
    download.status === 'queued' || download.status === 'staged'
  ).length;
  const doneCount = downloads.filter(download => download.status === 'completed').length;
  const schedulerRunning = useSettingsStore(state => state.schedulerRunning);
  const schedulerActiveDownloadIds = useSettingsStore(state => state.schedulerActiveDownloadIds);
  const globalSpeedLimit = useSettingsStore(state => state.globalSpeedLimit);
  const previousSpeedLimit = useRef<string | null>(null);
  const maxConcurrentDownloads = useSettingsStore(state => state.maxConcurrentDownloads);

  const acknowledgePairingTokenChange = () => {
    invoke('acknowledge_pairing_token_change').catch(error => {
      console.error('Failed to acknowledge pairing token migration notice:', error);
    });
  };

  const startSidebarResize = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = sidebarWidth;

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const nextWidth = Math.min(260, Math.max(190, startWidth + moveEvent.clientX - startX));
      setSidebarWidth(nextWidth);
    };

    const handlePointerUp = () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerUp);
      document.body.classList.remove('is-resizing');
    };

    document.body.classList.add('is-resizing');
    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp);
  };

  useEffect(() => {
    initMediaDomains();
    window.localStorage.setItem('firelink-sidebar-width', String(sidebarWidth));
  }, [sidebarWidth]);

  const { addToast } = useToast();

  useEffect(() => {
    useDownloadStore.getState().initDB();
    useSettingsStore.getState().hydratePairingToken()
      .then(changed => {
        if (changed) {
          addToast({
            variant: 'warning',
            isActionable: true,
            message: (
              <div className="flex flex-col gap-2">
                <p>Browser extension disconnected because its pairing token changed.</p>
                <div className="flex gap-2 justify-end">
                  <button
                    type="button"
                    className="app-button px-2 py-1 bg-surface-raised border border-border-color rounded"
                    onClick={async () => {
                      const token = useSettingsStore.getState().extensionPairingToken;
                      try {
                        if (token) {
                          await navigator.clipboard.writeText(token);
                        }
                        acknowledgePairingTokenChange();
                      } catch (error) {
                        addToast({
                          message: `Could not copy pairing token: ${String(error)}`,
                          variant: 'error',
                          isActionable: true
                        });
                      }
                    }}
                  >
                    Copy token
                  </button>
                  <button
                    type="button"
                    className="app-button px-2 py-1 bg-surface-raised border border-border-color rounded"
                    onClick={() => {
                      const settings = useSettingsStore.getState();
                      settings.setActiveSettingsTab('integrations');
                      settings.setActiveView('settings');
                      acknowledgePairingTokenChange();
                    }}
                  >
                    Integrations
                  </button>
                </div>
              </div>
            )
          });
        }
      })
      .catch(error => {
        console.error('Failed to hydrate extension pairing token:', error);
      });
  }, [addToast]);

  useEffect(() => {
    window.document.documentElement.setAttribute('data-font-size', appFontSize);
  }, [appFontSize]);

  useEffect(() => {
    window.document.documentElement.setAttribute('data-list-density', listRowDensity);
  }, [listRowDensity]);

  useEffect(() => {
    const checkForUpdate = () => {
      if (!useSettingsStore.getState().autoCheckUpdates || automaticUpdateCheckStarted) return;
      automaticUpdateCheckStarted = true;

      invoke('check_for_updates')
        .then(result => {
          if (result.type !== 'UpdateAvailable') return;
          addToast({
            variant: 'info',
            isActionable: true,
            message: (
              <div className="flex items-center gap-3">
                <span>Firelink {result.update.version} is available.</span>
                <button
                  type="button"
                  className="app-button px-2 py-1"
                  onClick={() => {
                    void openUrl(result.update.release_url);
                  }}
                >
                  View release
                </button>
              </div>
            )
          });
        })
        .catch(error => {
          automaticUpdateCheckStarted = false;
          console.error('Automatic update check failed:', error);
        });
    };

    if (useSettingsStore.persist.hasHydrated()) {
      checkForUpdate();
      return;
    }
    return useSettingsStore.persist.onFinishHydration(checkForUpdate);
  }, [addToast, autoCheckUpdates]);

  useEffect(() => {
    invoke('set_concurrent_limit', { limit: maxConcurrentDownloads }).catch(console.error);
  }, [maxConcurrentDownloads]);

  useEffect(() => {
    invoke('update_dock_badge', { count: showDockBadge ? activeDownloadCount : 0 }).catch(() => {});
  }, [showDockBadge, activeDownloadCount]);

  useEffect(() => {
    invoke('toggle_tray_icon', { show: showMenuBarIcon }).catch(console.error);
  }, [showMenuBarIcon]);

  useEffect(() => {
    if (!extensionPairingToken) return;
    invoke('set_extension_pairing_token', { token: extensionPairingToken }).catch(error => {
      console.error('Failed to configure browser extension pairing token:', error);
    });
  }, [extensionPairingToken]);

  useEffect(() => {
    if (previousSpeedLimit.current === globalSpeedLimit) return;
    previousSpeedLimit.current = globalSpeedLimit;
    
    const formattedLimit = normalizeSpeedLimitForBackend(globalSpeedLimit);

    invoke('set_global_speed_limit', { limit: formattedLimit }).catch(error => {
      console.error('Failed to apply global speed limit:', error);
    });
  }, [globalSpeedLimit]);

  useEffect(() => {
    const unlisten = listen('schedule-trigger', async (event) => {
      const state = useSettingsStore.getState();
      const payload = event.payload;
      if (processingScheduleKeys.has(payload.key)) return;
      processingScheduleKeys.add(payload.key);
      try {
        if (payload.action === 'start') {
          const startedResults = await Promise.all(
            getScheduledQueueIds().map(queueId => useDownloadStore.getState().startQueue(queueId))
          );
          const acceptedIds = startedResults.flat();
          state.setSchedulerActiveDownloadIds(acceptedIds);
          state.setSchedulerRunning(acceptedIds.length > 0);
          await invoke('ack_schedule_trigger', { action: 'start', key: payload.key });
        } else if (payload.action === 'stop') {
          await Promise.all(
            getScheduledQueueIds().map(queueId => useDownloadStore.getState().pauseQueue(queueId))
          );
          state.setSchedulerActiveDownloadIds([]);
          state.setSchedulerRunning(false);
          await invoke('ack_schedule_trigger', { action: 'stop', key: payload.key });
        }
      } finally {
        processingScheduleKeys.delete(payload.key);
      }
    });
    
    return () => {
      unlisten.then(f => f()).catch(console.error);
    };
  }, []);

  useEffect(() => {
    if (!schedulerRunning) return;
    if (schedulerActiveDownloadIds.length === 0) return;
    const scheduledIds = new Set(schedulerActiveDownloadIds);
    const hasPendingScheduledWork = downloads.some(download =>
      scheduledIds.has(download.id) && isActiveDownloadStatus(download.status)
    );
    if (hasPendingScheduledWork) return;

    const settings = useSettingsStore.getState();
    const scheduledItems = schedulerActiveDownloadIds.map(id =>
      downloads.find(download => download.id === id)
    );
    const hasFailures = scheduledItems.some(item => !item || item.status === 'failed');
    settings.setSchedulerActiveDownloadIds([]);
    settings.setSchedulerRunning(false);
    if (hasFailures) {
      addToast({
        message: 'Scheduled downloads finished with failures. The post-queue system action was skipped.',
        variant: 'warning',
        isActionable: true
      });
    } else if (settings.scheduler.postQueueAction !== 'none') {
      invoke('perform_system_action', { action: settings.scheduler.postQueueAction }).catch(error => {
        console.error('Scheduled post action failed:', error);
      });
    }
  }, [addToast, downloads, schedulerRunning, schedulerActiveDownloadIds]);

  useEffect(() => {
    const initNotifications = async () => {
      if (!useSettingsStore.getState().showNotifications) return;
      let permissionGranted = await isPermissionGranted();
      if (!permissionGranted) {
        await requestPermission();
      }
    };

    if (useSettingsStore.persist.hasHydrated()) {
      void initNotifications();
      return;
    }
    return useSettingsStore.persist.onFinishHydration(() => {
      void initNotifications();
    });
  }, [showNotifications]);

  useEffect(() => {
    const handlePaste = (e: ClipboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement ||
        (e.target as HTMLElement).isContentEditable
      ) {
        return;
      }
      const text = e.clipboardData?.getData('text/plain');
      if (text && text.trim().length > 0) {
        const urls = extractValidDownloadUrls(text);
        if (urls.length > 0) {
          useDownloadStore.getState().openAddModalWithUrls(urls.join('\n'));
        }
      }
    };
    window.addEventListener('paste', handlePaste);
    return () => window.removeEventListener('paste', handlePaste);
  }, []);

  useEffect(() => {
    const root = window.document.documentElement;
    
    const applyTheme = () => {
      // Remove all theme classes first
      root.classList.remove('theme-dark', 'theme-light', 'theme-dracula', 'theme-nord', 'dark');
      
      if (theme === 'system') {
        const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        root.classList.add(systemDark ? 'theme-dark' : 'theme-light');
        if (systemDark) root.classList.add('dark');
      } else {
        root.classList.add(`theme-${theme}`);
        if (['dark', 'dracula', 'nord'].includes(theme)) {
          root.classList.add('dark');
        }
      }
    };

    applyTheme();

    if (theme === 'system') {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const listener = () => applyTheme();
      mediaQuery.addEventListener('change', listener);
      return () => mediaQuery.removeEventListener('change', listener);
    }
  }, [theme]);

  useEffect(() => {
    initDownloadListener();

    const unlistenTerminalState = listen('download-state', (event) => {
      if (event.payload.status !== 'completed' && event.payload.status !== 'failed') return;
      const settings = useSettingsStore.getState();
      if (!settings.showNotifications) return;

      const item = useDownloadStore.getState().downloads.find(d => d.id === event.payload.id);
      const fileName = item?.fileName || 'A file';
      if (event.payload.status === 'completed') {
        sendNotification({
          title: 'Download Complete',
          body: `${fileName} has finished downloading.`,
          sound: settings.playCompletionSound ? 'default' : undefined
        });
      } else {
        sendNotification({
          title: 'Download Failed',
          body: `${fileName} failed to download.`,
        });
      }
    });

    const unlistenExtension = listen('extension-add-download', (event) => {
      useDownloadStore.getState().handleExtensionDownload(event.payload);
    });
    const unlistenDeepLink = listen('deep-link-add-download', (event) => {
      useDownloadStore.getState().openAddModalWithUrls(event.payload);
    });
    Promise.all([unlistenExtension, unlistenDeepLink])
      .then(() => invoke('set_extension_frontend_ready', { ready: true }))
      .catch(error => console.error('Failed to activate browser extension integration:', error));

    return () => {
      invoke('set_extension_frontend_ready', { ready: false }).catch(() => {});
      unlistenTerminalState.then(f => f());
      unlistenExtension.then(f => f());
      unlistenDeepLink.then(f => f());
    };
  }, []);

  return (
    <div className="app-shell flex h-screen w-screen overflow-hidden text-text-primary">
      <div
        className={`app-sidebar-shell relative z-20 shrink-0 transition-all duration-300 ease-[cubic-bezier(0.2,0.8,0.2,1)] ${
          isSidebarVisible ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        style={{ 
          width: sidebarWidth,
          marginLeft: isSidebarVisible ? 0 : -sidebarWidth
        }}
      >
        <div className="app-sidebar-panel h-full w-full">
          <Sidebar
            selectedFilter={filter}
            onSelectFilter={(f) => {
              setFilter(f);
              useSettingsStore.getState().setActiveView('downloads');
            }}
          />
        </div>
        <div
          className="sidebar-resize-handle"
          onPointerDown={startSidebarResize}
          title="Resize Sidebar"
        />
      </div>

      <div className="app-workspace relative z-0 flex-1 flex flex-col h-full overflow-hidden">
        <div className="flex-1 flex flex-col overflow-hidden relative">
          {activeView === 'downloads' && <DownloadTable filter={filter} />}
          {activeView === 'settings' && <SettingsView />}
          {activeView === 'scheduler' && <SchedulerView />}
          {activeView === 'speedLimiter' && <SpeedLimiterView />}
          {activeView === 'logs' && <LogsView />}
        </div>
        
        {/* Status Bar */}
        <div className="app-statusbar px-[14px] flex items-center justify-between text-text-muted shrink-0">
          <span>Ready</span>
          <div className="flex gap-3 tabular-nums">
            <span>{activeDownloadCount} active</span>
            <span>{queuedCount} queued</span>
            <span>{doneCount} done</span>
          </div>
        </div>
      </div>
      
      <AddDownloadsModal />
      <PropertiesModal />
      <DeleteConfirmationModal />

    </div>
  );
}

export default App;
