import { initMediaDomains } from './utils/downloads';
import { useEffect, useRef, useState } from "react";
import { Sidebar, SidebarFilter } from "./components/Sidebar";
import { DownloadTable } from "./components/DownloadTable";
import { AddDownloadsModal } from "./components/AddDownloadsModal";
import SettingsView from "./components/SettingsView";
import { PropertiesModal } from "./components/PropertiesModal";
import { QualityModal } from './components/QualityModal';
import { DeleteConfirmationModal } from "./components/DeleteConfirmationModal";
import { listenEvent as listen, invokeCommand as invoke } from "./ipc";
import { useDownloadStore, MAIN_QUEUE_ID } from './store/useDownloadStore';
import { initDownloadListener } from './store/downloadStore';
import { useSettingsStore } from "./store/useSettingsStore";
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import SchedulerView from "./components/SchedulerView";
import SpeedLimiterView from "./components/SpeedLimiterView";

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
  const showDockBadge = useSettingsStore(state => state.showDockBadge);
  const showMenuBarIcon = useSettingsStore(state => state.showMenuBarIcon);
  const extensionPairingToken = useSettingsStore(state => state.extensionPairingToken);
  const downloads = useDownloadStore(state => state.downloads);
  const activeDownloadCount = downloads.filter(download => download.status === 'downloading').length;
  const queuedCount = downloads.filter(download => download.status === 'queued').length;
  const doneCount = downloads.filter(download => download.status === 'completed').length;
  const schedulerRunning = useSettingsStore(state => state.schedulerRunning);
  const globalSpeedLimit = useSettingsStore(state => state.globalSpeedLimit);
  const previousSpeedLimit = useRef<string | null>(null);
  const maxConcurrentDownloads = useSettingsStore(state => state.maxConcurrentDownloads);

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

  useEffect(() => {
    useDownloadStore.getState().initDB();
  }, []);

  useEffect(() => {
    window.document.documentElement.setAttribute('data-font-size', appFontSize);
  }, [appFontSize]);

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
    invoke('set_extension_pairing_token', { token: extensionPairingToken }).catch(error => {
      console.error('Failed to configure browser extension pairing token:', error);
    });
  }, [extensionPairingToken]);

  useEffect(() => {
    if (previousSpeedLimit.current === globalSpeedLimit) return;
    previousSpeedLimit.current = globalSpeedLimit;
    
    // Convert to aria2 format (e.g. "1M", "500K")
    let formattedLimit = null;
    if (globalSpeedLimit) {
      const match = globalSpeedLimit.trim().match(/^(\d+(?:\.\d+)?)\s*([kmgt]?)b?(?:\/s)?$/i);
      if (match) {
        const amount = Number(match[1]);
        if (Number.isFinite(amount) && amount > 0) {
           const multipliers: Record<string, number> = { '': 1, k: 1024, m: 1048576, g: 1073741824, t: 1099511627776 };
           const bytes = Math.round(amount * multipliers[match[2].toLowerCase()]);
           formattedLimit = `${bytes}`;
        }
      }
    }

    invoke('set_global_speed_limit', { limit: formattedLimit }).catch(error => {
      console.error('Failed to apply global speed limit:', error);
    });
  }, [globalSpeedLimit]);

  useEffect(() => {
    const unlisten = listen('schedule-trigger', async (event) => {
      const state = useSettingsStore.getState();
      if (event.payload === 'start') {
        const started = await useDownloadStore.getState().startQueue(MAIN_QUEUE_ID);
        state.setSchedulerRunning(started > 0);
      } else if (event.payload === 'stop') {
        await useDownloadStore.getState().pauseQueue(MAIN_QUEUE_ID);
        state.setSchedulerRunning(false);
      }
    });
    
    return () => {
      unlisten.then(f => f()).catch(console.error);
    };
  }, []);

  useEffect(() => {
    if (!schedulerRunning) return;
    const hasPendingScheduledWork = downloads.some(download =>
      download.status === 'queued' || download.status === 'downloading'
    );
    if (hasPendingScheduledWork) return;

    const settings = useSettingsStore.getState();
    settings.setSchedulerRunning(false);
    if (settings.scheduler.postQueueAction !== 'none') {
      invoke('perform_system_action', { action: settings.scheduler.postQueueAction }).catch(error => {
        console.error('Scheduled post action failed:', error);
      });
    }
  }, [downloads, schedulerRunning]);

  useEffect(() => {
    // Request notification permissions
    const initNotifications = async () => {
      let permissionGranted = await isPermissionGranted();
      if (!permissionGranted) {
        await requestPermission();
      }
    };
    initNotifications();
  }, []);

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
        useDownloadStore.getState().openAddModalWithUrls(text.trim());
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

    const unlistenComplete = listen('download-complete', (event) => {
      const settings = useSettingsStore.getState();
      if (settings.showNotifications) {
        const item = useDownloadStore.getState().downloads.find(d => d.id === event.payload);
        const fileName = item?.fileName || 'A file';
        
        sendNotification({
          title: 'Download Complete',
          body: `${fileName} has finished downloading.`,
          sound: settings.playCompletionSound ? 'default' : undefined
        });
      }
    });

    const unlistenFailed = listen('download-failed', (event) => {
      const settings = useSettingsStore.getState();
      if (settings.showNotifications) {
        const item = useDownloadStore.getState().downloads.find(d => d.id === event.payload);
        const fileName = item?.fileName || 'A file';
        
        sendNotification({
          title: 'Download Failed',
          body: `${fileName} failed to download.`,
        });
      }
    });

    const unlistenExtension = listen('extension-add-download', (event) => {
      useDownloadStore.getState().handleExtensionDownload(event.payload);
    });
    const unlistenExtensionQueued = listen('extension-downloads-queued', (event) => {
      const store = useDownloadStore.getState();
      const incoming = event.payload;
      const existing = new Set(store.downloads.map(download => download.id));
      const additions = incoming.filter(download => !existing.has(download.id));
      if (additions.length === 0) return;
      useDownloadStore.setState(state => ({
        downloads: [...state.downloads, ...additions],
        pendingOrder: [
          ...state.pendingOrder,
          ...additions
            .filter(download => download.status === 'queued')
            .map(download => download.id)
            .filter(id => !state.pendingOrder.includes(id)),
        ],
      }));
    });
    const unlistenDeepLink = listen('deep-link-add-download', (event) => {
      useDownloadStore.getState().openAddModalWithUrls(event.payload);
    });
    Promise.all([unlistenExtension, unlistenExtensionQueued, unlistenDeepLink])
      .then(() => invoke('set_extension_frontend_ready', { ready: true }))
      .catch(error => console.error('Failed to activate browser extension integration:', error));

    return () => {
      invoke('set_extension_frontend_ready', { ready: false }).catch(() => {});
      unlistenComplete.then(f => f());
      unlistenFailed.then(f => f());
      unlistenExtension.then(f => f());
      unlistenExtensionQueued.then(f => f());
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
      <QualityModal />
      <DeleteConfirmationModal />
    </div>
  );
}

export default App;
