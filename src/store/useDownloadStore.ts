import { create } from 'zustand';
import { LazyStore } from '@tauri-apps/plugin-store';
import { info } from '@tauri-apps/plugin-log';
import { invokeCommand as invoke } from '../ipc';

export const tauriStore = new LazyStore('store.bin');
import type { DownloadItem } from '../bindings/DownloadItem';
import type { DownloadStatus } from '../bindings/DownloadStatus';
import type { ExtensionDownload } from '../bindings/ExtensionDownload';
import type { Queue } from '../bindings/Queue';
import type { MediaMetadata } from '../bindings/MediaMetadata';
import { useSettingsStore } from './useSettingsStore';

export type { DownloadCategory } from '../utils/downloads';

const getProxyArgs = async (settings: ReturnType<typeof useSettingsStore.getState>) => {
  if (settings.proxyMode === 'system') {
    try {
      return await invoke('get_system_proxy');
    } catch (e) {
      console.warn("Failed to get system proxy:", e);
      return null;
    }
  }
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
        const escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
        const regex = new RegExp('^' + escaped.replace(/\*/g, '.*') + '$');
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

// Legacy manual speed limit math removed

export type { DownloadStatus };
export const MAIN_QUEUE_ID = '00000000-0000-0000-0000-000000000001';

export type { DownloadItem, Queue };
export type ExtensionDownloadRequest = ExtensionDownload;

export type DeleteModalState = {
  isOpen: boolean;
  downloadId?: string;
};

interface DownloadState {
  downloads: DownloadItem[];
  queues: Queue[];
  isAddModalOpen: boolean;
  pendingAddUrls: string;
  pendingAddReferer: string;
  pendingAddFilename: string;
  selectedPropertiesDownloadId: string | null;
  toggleAddModal: (isOpen: boolean) => void;
  openAddModalWithUrls: (urls: string, referer?: string | null, filename?: string | null) => void;
  handleExtensionDownload: (request: ExtensionDownloadRequest) => void;
  deleteModalState: DeleteModalState;
  openDeleteModal: (downloadId?: string) => void;
  closeDeleteModal: () => void;
  setSelectedPropertiesDownloadId: (id: string | null) => void;
  addDownload: (item: DownloadItem) => void;
  updateDownload: (id: string, updates: Partial<DownloadItem>) => void;
  removeDownload: (id: string, deleteFile?: boolean) => Promise<void>;
  redownload: (id: string) => void;
  processQueue: () => Promise<void>;
  startQueue: (queueId: string) => Promise<number>;
  pauseQueue: (queueId: string) => Promise<number>;
  addQueue: (name: string) => void;
  renameQueue: (id: string, name: string) => void;
  removeQueue: (id: string) => void;
  initDB: () => Promise<void>;
  
  isParsing: boolean;
  activeMetadata: MediaMetadata | null;
  activeMetadataUrl: string | null;
  parsingError: string | null;
  fetchMetadataAction: (url: string) => Promise<void>;
  clearMetadata: () => void;
}

let isProcessingQueue = false;

export const useDownloadStore = create<DownloadState>((set, get) => ({
  downloads: [],
  queues: [{ id: MAIN_QUEUE_ID, name: 'Main Queue', isMain: true }],
  isAddModalOpen: false,
  pendingAddUrls: '',
  pendingAddReferer: '',
  pendingAddFilename: '',
  selectedPropertiesDownloadId: null,
  isParsing: false,
  activeMetadata: null,
  activeMetadataUrl: null,
  parsingError: null,
  deleteModalState: { isOpen: false },
  openDeleteModal: (downloadId) => set({ deleteModalState: { isOpen: true, downloadId } }),
  closeDeleteModal: () => set({ deleteModalState: { isOpen: false } }),
  toggleAddModal: (isOpen) => set({
    isAddModalOpen: isOpen,
    pendingAddUrls: '',
    pendingAddReferer: '',
    pendingAddFilename: ''
  }),
  openAddModalWithUrls: (urls, referer, filename) => set((state) => {
    const existingUrls = state.isAddModalOpen && state.pendingAddUrls ? state.pendingAddUrls : '';
    const mergedUrls = existingUrls ? `${existingUrls}\n${urls}` : urls;
    return {
      isAddModalOpen: true,
      pendingAddUrls: mergedUrls,
      pendingAddReferer: referer?.trim() || state.pendingAddReferer || '',
      pendingAddFilename: filename?.trim() || state.pendingAddFilename || ''
    };
  }),
  handleExtensionDownload: (request) => {
    const urls = [...new Set(request.urls.map(url => url.trim()).filter(Boolean))];
    if (urls.length === 0) return;

    get().openAddModalWithUrls(
      urls.join('\n'),
      request.referer,
      urls.length === 1 ? request.filename : null
    );
  },
  setSelectedPropertiesDownloadId: (id) => set({ selectedPropertiesDownloadId: id }),
  clearMetadata: () => set({ isParsing: false, activeMetadata: null, activeMetadataUrl: null, parsingError: null }),
  fetchMetadataAction: async (url) => {
    set({ isParsing: true, parsingError: null, activeMetadata: null, activeMetadataUrl: url });
    try {
      const settings = useSettingsStore.getState();
      const metadata = await invoke('fetch_media_metadata', { 
        url,
        cookieBrowser: settings.mediaCookieSource === 'none' ? null : settings.mediaCookieSource,
        username: null,
        password: null
      });
      set({ isParsing: false, activeMetadata: metadata });
      info(`Media metadata parsed for ${url}: found ${metadata.formats.length} formats`);
    } catch (e) {
      set({ isParsing: false, parsingError: String(e) });
      info(`Media metadata parsing failed for ${url}: ${e}`);
    }
  },
  addDownload: (item) => {
    info(`Download ${item.id} added to queue`);
    set((state) => ({ downloads: [...state.downloads, item] }));
    get().processQueue();
  },
  updateDownload: (id, updates) => {
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          const updated = { 
            ...d, 
            ...updates,
            fraction: updates.fraction !== undefined ? updates.fraction : d.fraction
          };
          return updated;
        }
        return d;
      })
    }));
    
    // If status changed to something that frees up a slot, process queue
    if (updates.status && ['completed', 'failed', 'paused'].includes(updates.status)) {
      info(`Download ${id} status changed to ${updates.status}`);
      get().processQueue();
      syncSystemIntegrations();
    } else if (updates.status === 'downloading') {
      info(`Download ${id} status changed to downloading`);
      syncSystemIntegrations();
    }
  },
  removeDownload: async (id, deleteFile = false) => {
    const item = get().downloads.find(d => d.id === id);
    if (item && item.status === 'downloading') {
      try {
        const filepath = item.destination ? `${item.destination}/${item.fileName}` : null;
        await invoke('remove_download', { id, filepath });
      } catch (e) {
        console.error("Failed to terminate download on deletion:", e);
      }
    } else if (item && deleteFile) {
      try {
        const filepath = item.destination ? `${item.destination}/${item.fileName}` : null;
        await invoke('remove_download', { id, filepath });
      } catch (e) {
        console.error("Failed to delete file from disk:", e);
      }
    }
    set((state) => ({
      downloads: state.downloads.filter(d => d.id !== id)
    }));
    info(`Download ${id} removed`);
    get().processQueue();
    syncSystemIntegrations();
  },
  redownload: (id) => {
    let wasDownloading = false;
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          if (d.status === 'downloading') {
            wasDownloading = true;
          }
          const updated: DownloadItem = { ...d, status: 'queued', _dispatched: false, fraction: 0, speed: '-', eta: '-' };
          return updated;
        }
        return d;
      })
    }));
    if (wasDownloading) {
      invoke('pause_download', { id }).catch(console.error);
    }
    info(`Download ${id} redownload requested (queued)`);
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
          ? { ...item, status: 'queued', _dispatched: false, speed: '-', eta: '-' }
          : item
      )
    }));

    info(`Queue ${queueId} started, ${runnableIds.length} items queued`);
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

    info(`Queue ${queueId} paused, ${activeIds.length} items paused`);
    await Promise.all(activeIds.map(id => invoke('pause_download', { id }).catch(() => {})));
    syncSystemIntegrations();
    return activeIds.length;
  },
  addQueue: (name) => {
    const id = crypto.randomUUID();
    const q = { id, name, isMain: false };
    set((state) => ({
      queues: [...state.queues, q]
    }));
  },
  renameQueue: (id, name) => {
    set((state) => ({
      queues: state.queues.map(q => {
        if (q.id === id) {
          const newQ = { ...q, name };
          return newQ;
        }
        return q;
      })
    }));
  },
  removeQueue: (id) => {
    if (id === MAIN_QUEUE_ID) return;
    
    set((state) => ({
      queues: state.queues.filter(q => q.id !== id),
      downloads: state.downloads.map(d => 
        d.queueId === id ? { ...d, queueId: MAIN_QUEUE_ID } : d
      )
    }));
  },
  initDB: async () => {
    try {
      const queues = await tauriStore.get<Queue[]>('queues') || [];
      const downloads = await tauriStore.get<DownloadItem[]>('download_queue') || [];
      
      set(state => ({
        queues: queues.length > 0 ? queues : state.queues,
        downloads: downloads.length > 0 ? downloads : state.downloads
      }));
      
      // Auto resume downloads that were active
      const active = get().downloads.filter(d => d.status === 'downloading');
      const settings = useSettingsStore.getState();
      active.forEach(item => {
        if (item.isMedia) {
          invoke('start_media_download', {
            id: item.id,
            url: item.url,
            destination: item.destination || '~/Downloads',
            filename: item.fileName,
            formatSelector: item.mediaFormatSelector || null,
            cookieSource: null,
            speedLimit: item.speedLimit || settings.globalSpeedLimit || null,
            username: item.username || null,
            password: item.password || null,
            headers: item.headers || null,
            proxy: null,
            userAgent: null,
            maxTries: null
          }).catch(console.error);
        } else {
          invoke('start_download', {
            id: item.id,
            url: item.url,
            destination: item.destination || '~/Downloads',
            filename: item.fileName,
            connections: item.connections ?? null,
            speedLimit: item.speedLimit || settings.globalSpeedLimit || null,
            username: item.username || null,
            password: item.password || null,
            headers: item.headers || null,
            checksum: item.checksum || null,
            cookies: item.cookies || null,
            mirrors: item.mirrors || null,
            userAgent: null,
            maxTries: null,
            proxy: null
          }).catch(console.error);
        }
      });
      
      void get().processQueue();
    } catch (e) {
      console.error("Failed to init DB", e);
    }
  },
  processQueue: async () => {
    if (isProcessingQueue) return;
    isProcessingQueue = true;
    
    try {
      const { downloads, updateDownload } = get();
      const settings = useSettingsStore.getState();
      const concurrentLimit = settings.maxConcurrentDownloads || 3;
      const activeCount = downloads.filter(d => d.status === 'downloading').length;
      let availableSlots = concurrentLimit - activeCount;

      if (availableSlots <= 0) return;

      const itemsToStart = downloads.filter(d => d.status === 'queued' && !d._dispatched);
      
      for (const item of itemsToStart) {
        if (availableSlots <= 0) break;
        availableSlots--;

        // Mark as dispatched so we don't send it again on the next pass
        updateDownload(item.id, { _dispatched: true });
        try {
          const login = getSiteLogin(item.url, settings);
          let keychainPassword = null;
          if (login) {
            try {
              keychainPassword = await invoke('get_keychain_password', { id: login.id });
            } catch (e) {
              console.warn("Could not fetch keychain password for login:", e);
            }
          }
          
          const destPath = item.destination || 
                           (settings.downloadDirectories && settings.downloadDirectories[item.category]) || 
                           settings.defaultDownloadPath || 
                           '~/Downloads';

          if (item.isMedia) {
            await invoke('start_media_download', {
              id: item.id,
              url: item.url,
              destination: destPath,
              filename: item.fileName,
              formatSelector: item.mediaFormatSelector || null,
              cookieSource: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
              speedLimit: item.speedLimit || settings.globalSpeedLimit || null,
              username: item.username || (login ? login.username : null),
              password: item.password || keychainPassword,
              headers: item.headers || null,
              proxy: await getProxyArgs(settings),
              userAgent: settings.customUserAgent || null,
              maxTries: settings.maxAutomaticRetries
            });
          } else {
            await invoke('start_download', {
              id: item.id,
              url: item.url,
              destination: destPath,
              filename: item.fileName,
              connections: item.connections || settings.perServerConnections || null,
              speedLimit: item.speedLimit || settings.globalSpeedLimit || null,
              username: item.username || (login ? login.username : null),
              password: item.password || keychainPassword,
              headers: item.headers || null,
              checksum: item.checksum || null,
              cookies: item.cookies || null,
              mirrors: item.mirrors || null,
              userAgent: settings.customUserAgent || null,
              maxTries: settings.maxAutomaticRetries,
              proxy: await getProxyArgs(settings)
            });
          }
        } catch (e) {
          console.error("Failed to start queued download:", e);
          updateDownload(item.id, { status: 'failed' });
        }
      }
    } finally {
      isProcessingQueue = false;
    }
  }
}));

let lastSavedDownloads = '';

useDownloadStore.subscribe(async (state, prevState) => {
  if (state.queues !== prevState.queues) {
    await tauriStore.set('queues', state.queues);
    await tauriStore.save();
  }

  if (state.downloads !== prevState.downloads) {
    const staticDownloads = state.downloads.map(d => {
      const copy = { ...d };
      delete copy.fraction;
      delete copy.speed;
      delete copy.eta;
      delete copy._dispatched;
      return copy;
    });
    
    const currentSerialized = JSON.stringify(staticDownloads);
    if (currentSerialized !== lastSavedDownloads) {
      lastSavedDownloads = currentSerialized;
      await tauriStore.set('download_queue', staticDownloads);
      await tauriStore.save();
    }
  }
});
