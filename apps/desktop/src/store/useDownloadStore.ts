import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { useSettingsStore } from './useSettingsStore';

const getProxyArgs = (settings: ReturnType<typeof useSettingsStore.getState>) => {
  if (settings.proxyMode === 'custom' && settings.proxyHost) {
    return `http://${settings.proxyHost}:${settings.proxyPort}`;
  }
  return null;
};

export const getSiteLogin = (url: string, settings: ReturnType<typeof useSettingsStore.getState>) => {
  try {
    const urlObj = new URL(url);
    const host = urlObj.hostname.toLowerCase();
    for (const login of settings.siteLogins) {
      let pattern = login.urlPattern.toLowerCase().trim();
      if (pattern.startsWith('*.')) {
        const suffix = pattern.substring(2);
        if (host === suffix || host.endsWith('.' + suffix)) return login;
      } else if (pattern.includes('*')) {
        const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
        if (regex.test(host)) return login;
      } else if (host === pattern) {
        return login;
      }
    }
  } catch (e) {}
  return null;
};

const syncSystemIntegrations = () => {
  const settings = useSettingsStore.getState();
  const activeCount = useDownloadStore.getState().downloads.filter(d => d.status === 'downloading').length;
  invoke('update_dock_badge', { count: settings.showDockBadge ? activeCount : 0 }).catch(() => {});
  if (settings.preventsSleepWhileDownloading) {
    invoke('set_prevent_sleep', { prevent: activeCount > 0 }).catch(() => {});
  } else {
    invoke('set_prevent_sleep', { prevent: false }).catch(() => {});
  }
};

const speedLimitToKiB = (value?: string | null): number | null => {
  if (!value) return null;
  const match = value.trim().match(/^(\d+(?:\.\d+)?)\s*([kmgt]?)b?(?:\/s)?$/i);
  if (!match) return null;

  const amount = Number(match[1]);
  if (!Number.isFinite(amount) || amount <= 0) return null;

  const multipliers: Record<string, number> = {
    '': 1,
    k: 1,
    m: 1024,
    g: 1024 * 1024,
    t: 1024 * 1024 * 1024
  };
  return Math.max(1, Math.round(amount * multipliers[match[2].toLowerCase()]));
};

const effectiveSpeedLimit = (
  itemLimit: string | null | undefined,
  globalLimit: string,
  maxConcurrentDownloads: number
): string | null => {
  const itemKiB = speedLimitToKiB(itemLimit);
  const globalKiB = speedLimitToKiB(globalLimit);
  const perSlotGlobalKiB = globalKiB
    ? Math.max(1, Math.floor(globalKiB / Math.max(maxConcurrentDownloads, 1)))
    : null;

  const effectiveKiB = itemKiB && perSlotGlobalKiB
    ? Math.min(itemKiB, perSlotGlobalKiB)
    : itemKiB ?? perSlotGlobalKiB;

  return effectiveKiB ? `${effectiveKiB}K` : null;
};

export type DownloadStatus = 'downloading' | 'paused' | 'completed' | 'failed' | 'queued';
export type DownloadCategory = 'Musics' | 'Movies' | 'Compressed' | 'Documents' | 'Pictures' | 'Applications' | 'Other';

export const MAIN_QUEUE_ID = '00000000-0000-0000-0000-000000000001';

export interface Queue {
  id: string;
  name: string;
  isMain: boolean;
}

export interface DownloadItem {
  id: string;
  url: string;
  fileName: string;
  status: DownloadStatus;
  fraction?: number;
  speed?: string;
  eta?: string;
  size?: string;
  category: DownloadCategory;
  dateAdded: string;
  // Advanced Settings
  connections?: number | null;
  speedLimit?: string | null;
  username?: string | null;
  password?: string | null;
  headers?: string | null;
  checksum?: string | null;
  cookies?: string | null;
  mirrors?: string | null;
  destination?: string;
  isMedia?: boolean;
  mediaFormatSelector?: string;
  queueId: string;
}

interface DownloadState {
  downloads: DownloadItem[];
  queues: Queue[];
  isAddModalOpen: boolean;
  pendingAddUrls: string;
  selectedPropertiesDownloadId: string | null;
  toggleAddModal: (isOpen: boolean) => void;
  openAddModalWithUrls: (urls: string) => void;
  setSelectedPropertiesDownloadId: (id: string | null) => void;
  addDownload: (item: DownloadItem) => void;
  updateDownload: (id: string, updates: Partial<DownloadItem>) => void;
  removeDownload: (id: string) => Promise<void>;
  clearFinished: () => void;
  redownload: (id: string) => void;
  processQueue: () => Promise<void>;
  startQueue: (queueId: string) => Promise<number>;
  pauseQueue: (queueId: string) => Promise<number>;
  addQueue: (name: string) => void;
  renameQueue: (id: string, name: string) => void;
  removeQueue: (id: string) => void;
  restartActiveDownloads: () => Promise<number>;
}

export const useDownloadStore = create<DownloadState>((set, get) => ({
  downloads: [],
  queues: [{ id: MAIN_QUEUE_ID, name: 'Main Queue', isMain: true }],
  isAddModalOpen: false,
  pendingAddUrls: '',
  selectedPropertiesDownloadId: null,
  toggleAddModal: (isOpen) => set({ isAddModalOpen: isOpen }),
  openAddModalWithUrls: (urls) => set({ isAddModalOpen: true, pendingAddUrls: urls }),
  setSelectedPropertiesDownloadId: (id) => set({ selectedPropertiesDownloadId: id }),
  addDownload: (item) => {
    set((state) => ({ downloads: [...state.downloads, item] }));
    get().processQueue();
  },
  updateDownload: (id, updates) => {
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          let newFraction = updates.fraction;
          if (newFraction === 0 && d.fraction && d.fraction > 0) {
            newFraction = d.fraction;
          }
          return { 
            ...d, 
            ...updates,
            fraction: newFraction !== undefined ? newFraction : updates.fraction !== undefined ? updates.fraction : d.fraction
          };
        }
        return d;
      })
    }));
    
    // If status changed to something that frees up a slot, process queue
    if (updates.status && ['completed', 'failed', 'paused'].includes(updates.status)) {
      get().processQueue();
      syncSystemIntegrations();
    } else if (updates.status === 'downloading') {
      syncSystemIntegrations();
    }
  },
  removeDownload: async (id) => {
    const item = get().downloads.find(d => d.id === id);
    if (item && item.status === 'downloading') {
      try {
        await invoke('pause_download', { id });
      } catch (e) {
        console.error("Failed to terminate download on deletion:", e);
      }
    }
    set((state) => ({
      downloads: state.downloads.filter(d => d.id !== id)
    }));
    get().processQueue();
    syncSystemIntegrations();
  },
  clearFinished: () => {
    set((state) => ({
      downloads: state.downloads.filter(d => !['completed', 'failed'].includes(d.status))
    }));
  },
  redownload: (id) => {
    set((state) => ({
      downloads: state.downloads.map(d => 
        d.id === id 
          ? { ...d, status: 'queued', fraction: 0, speed: '-', eta: '-' } 
          : d
      )
    }));
    get().processQueue();
  },
  startQueue: async (queueId) => {
    const runnableIds = get().downloads
      .filter(item => item.queueId === queueId && (item.status === 'queued' || item.status === 'paused' || item.status === 'failed'))
      .map(item => item.id);

    if (runnableIds.length === 0) return 0;

    set((state) => ({
      downloads: state.downloads.map(item =>
        runnableIds.includes(item.id)
          ? { ...item, status: 'queued', speed: '-', eta: '-' }
          : item
      )
    }));
    await get().processQueue();
    return runnableIds.length;
  },
  pauseQueue: async (queueId) => {
    const activeIds = get().downloads
      .filter(item => item.queueId === queueId && item.status === 'downloading')
      .map(item => item.id);

    if (activeIds.length === 0) return 0;

    set((state) => ({
      downloads: state.downloads.map(item =>
        activeIds.includes(item.id)
          ? { ...item, status: 'paused', speed: '-', eta: '-' }
          : item
      )
    }));
    await Promise.all(activeIds.map(id => invoke('pause_download', { id }).catch(() => {})));
    syncSystemIntegrations();
    return activeIds.length;
  },
  addQueue: (name) => {
    set((state) => ({ queues: [...state.queues, { id: crypto.randomUUID(), name, isMain: false }] }));
  },
  renameQueue: (id, name) => {
    set((state) => ({
      queues: state.queues.map(q => q.id === id ? { ...q, name } : q)
    }));
  },
  removeQueue: (id) => {
    set((state) => ({
      queues: state.queues.filter(q => q.id !== id || q.isMain),
      downloads: state.downloads.map(d => d.queueId === id ? { ...d, queueId: MAIN_QUEUE_ID } : d)
    }));
  },
  restartActiveDownloads: async () => {
    const activeIds = get().downloads
      .filter(item => item.status === 'downloading')
      .map(item => item.id);

    if (activeIds.length === 0) return 0;

    set((state) => ({
      downloads: state.downloads.map(item =>
        activeIds.includes(item.id)
          ? { ...item, status: 'paused', speed: '-', eta: '-' }
          : item
      )
    }));
    await Promise.all(activeIds.map(id => invoke('pause_download', { id }).catch(() => {})));
    await new Promise(resolve => window.setTimeout(resolve, 350));
    set((state) => ({
      downloads: state.downloads.map(item =>
        activeIds.includes(item.id)
          ? { ...item, status: 'queued' }
          : item
      )
    }));
    await get().processQueue();
    return activeIds.length;
  },
  processQueue: async () => {
    const { downloads, updateDownload } = get();
    const settingsSnapshot = useSettingsStore.getState();
    const { maxConcurrentDownloads } = settingsSnapshot;
    
    const activeCount = downloads.filter(d => d.status === 'downloading').length;
    if (activeCount >= maxConcurrentDownloads) return;

    const queuedItems = downloads.filter(d => d.status === 'queued');
    const slotsAvailable = maxConcurrentDownloads - activeCount;
    
    const itemsToStart = queuedItems.slice(0, slotsAvailable);
    
    for (const item of itemsToStart) {
      updateDownload(item.id, { status: 'downloading' });
      try {
        const settings = useSettingsStore.getState();
        const login = getSiteLogin(item.url, settings);
        let keychainPassword = null;
        if (login) {
          try {
            keychainPassword = await invoke<string>('get_keychain_password', { id: login.id });
          } catch (e) {
            console.warn("Could not fetch keychain password for login:", e);
          }
        }
        
        const destPath = item.destination || 
                         (settings.downloadDirectories && settings.downloadDirectories[item.category]) || 
                         settings.defaultDownloadPath || 
                         '~/Downloads';

        if (item.isMedia) {
          const speedLimit = effectiveSpeedLimit(
            item.speedLimit,
            settings.globalSpeedLimit,
            settings.maxConcurrentDownloads
          );
          await invoke('start_media_download', {
            id: item.id,
            url: item.url,
            destination: destPath,
            filename: item.fileName,
            formatSelector: item.mediaFormatSelector || null,
            cookieSource: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
            speedLimit,
            username: item.username || (login ? login.username : null),
            password: item.password || keychainPassword
          });
        } else {
          const speedLimit = effectiveSpeedLimit(
            item.speedLimit,
            settings.globalSpeedLimit,
            settings.maxConcurrentDownloads
          );
          await invoke('start_download', {
            id: item.id,
            url: item.url,
            destination: destPath,
            filename: item.fileName,
            connections: item.connections || settings.perServerConnections || null,
            speedLimit,
            username: item.username || (login ? login.username : null),
            password: item.password || keychainPassword,
            headers: item.headers || null,
            checksum: item.checksum || null,
            cookies: item.cookies || null,
            mirrors: item.mirrors || null,
            userAgent: settings.customUserAgent || null,
            maxTries: settings.maxAutomaticRetries,
            proxy: getProxyArgs(settings)
          });
        }
      } catch (e) {
        console.error("Failed to start queued download:", e);
        updateDownload(item.id, { status: 'failed' });
      }
    }
  }
}));
