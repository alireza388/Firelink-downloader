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
  pendingOrder: string[];
  setPendingOrder: (order: string[]) => void;
  moveInQueue: (id: string, direction: 'Up' | 'Down') => Promise<void>;
  removeFromQueue: (id: string) => Promise<void>;
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
  addDownload: (item: DownloadItem) => Promise<void>;
  updateDownload: (id: string, updates: Partial<DownloadItem>) => void;
  removeDownload: (id: string, deleteFile?: boolean) => Promise<void>;
  redownload: (id: string) => Promise<void>;
  resumeDownload: (id: string) => Promise<void>;
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

export const useDownloadStore = create<DownloadState>((set, get) => ({
  downloads: [],
  queues: [{ id: MAIN_QUEUE_ID, name: 'Main Queue', isMain: true }],
  pendingOrder: [],
  setPendingOrder: (order) => set({ pendingOrder: order }),
  moveInQueue: async (id, direction) => {
    try {
      const order = await invoke('move_in_queue', { id, direction });
      set({ pendingOrder: order });
    } catch (e) {
      console.error("Failed to move item in queue:", e);
    }
  },
  removeFromQueue: async (id) => {
    try {
      await invoke('remove_from_queue', { id });
      set((state) => ({
        pendingOrder: state.pendingOrder.filter(x => x !== id)
      }));
    } catch (e) {
      console.error("Failed to remove item from queue:", e);
    }
  },
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
  addDownload: async (item) => {
    info(`Download ${item.id} added to queue`);
    set((state) => ({ downloads: [...state.downloads, item] }));

    try {
      const settings = useSettingsStore.getState();
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
                       
      const enqueueItem = {
        id: item.id,
        url: item.url,
        destination: destPath,
        filename: item.fileName,
        connections: item.connections || settings.perServerConnections || null,
        speed_limit: item.speedLimit || settings.globalSpeedLimit || null,
        username: item.username || (login ? login.username : null),
        password: item.password || keychainPassword,
        headers: item.headers || null,
        checksum: item.checksum || null,
        cookies: item.cookies || null,
        mirrors: item.mirrors || null,
        user_agent: settings.customUserAgent || null,
        max_tries: settings.maxAutomaticRetries,
        proxy: await getProxyArgs(settings),
        format_selector: item.mediaFormatSelector || null,
        cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
        is_media: item.isMedia || false
      };
      
      await invoke('enqueue_download', { item: enqueueItem });
      const order = await invoke('get_pending_order');
      set({ pendingOrder: order });
    } catch (e) {
      console.error("Failed to enqueue download:", e);
      get().updateDownload(item.id, { status: 'failed' });
    }
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
    
    if (updates.status && ['completed', 'failed', 'paused'].includes(updates.status)) {
      info(`Download ${id} status changed to ${updates.status}`);
      syncSystemIntegrations();
    } else if (updates.status === 'downloading') {
      info(`Download ${id} status changed to downloading`);
      syncSystemIntegrations();
    }
  },
  removeDownload: async (id, deleteFile = false) => {
    const item = get().downloads.find(d => d.id === id);
    if (item) {
      try {
        await invoke('remove_download', { id, filepath: null });
      } catch (e) {
        console.error("Failed to terminate download on deletion:", e);
      }
    } 
    
    if (item && deleteFile) {
      try {
        const filepath = item.destination ? `${item.destination}/${item.fileName}` : null;
        if (filepath) {
          const partialPaths = [`${filepath}.aria2`, `${filepath}.part`];
          await invoke('trash_download_assets', { path: filepath, partialPaths });
        }
      } catch (e) {
        console.error("Failed to trash file from disk:", e);
      }
    }
    set((state) => ({
      downloads: state.downloads.filter(d => d.id !== id),
      pendingOrder: state.pendingOrder.filter(x => x !== id)
    }));
    info(`Download ${id} removed`);
    syncSystemIntegrations();
  },
  redownload: async (id) => {
    let wasDownloading = false;
    let targetItem: DownloadItem | undefined;
    set((state) => {
      targetItem = state.downloads.find(d => d.id === id);
      if (targetItem && targetItem.status === 'downloading') {
        wasDownloading = true;
      }
      return {
        downloads: state.downloads.map(d => {
          if (d.id === id) {
            return { ...d, status: 'queued', fraction: 0, speed: '-', eta: '-' };
          }
          return d;
        })
      };
    });
    
    if (wasDownloading) {
      await invoke('pause_download', { id }).catch(console.error);
    }
    
    if (targetItem) {
      try {
        const settings = useSettingsStore.getState();
        const login = getSiteLogin(targetItem.url, settings);
        let keychainPassword = null;
        if (login) {
          try {
            keychainPassword = await invoke('get_keychain_password', { id: login.id });
          } catch (e) {
            console.warn("Could not fetch keychain password for login:", e);
          }
        }
        const destPath = targetItem.destination || 
                         (settings.downloadDirectories && settings.downloadDirectories[targetItem.category]) || 
                         settings.defaultDownloadPath || 
                         '~/Downloads';
        const enqueueItem = {
          id: targetItem.id,
          url: targetItem.url,
          destination: destPath,
          filename: targetItem.fileName,
          connections: targetItem.connections || settings.perServerConnections || null,
          speed_limit: targetItem.speedLimit || settings.globalSpeedLimit || null,
          username: targetItem.username || (login ? login.username : null),
          password: targetItem.password || keychainPassword,
          headers: targetItem.headers || null,
          checksum: targetItem.checksum || null,
          cookies: targetItem.cookies || null,
          mirrors: targetItem.mirrors || null,
          user_agent: settings.customUserAgent || null,
          max_tries: settings.maxAutomaticRetries,
          proxy: await getProxyArgs(settings),
          format_selector: targetItem.mediaFormatSelector || null,
          cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
          is_media: targetItem.isMedia || false
        };
        await invoke('enqueue_download', { item: enqueueItem });
        const order = await invoke('get_pending_order');
        set({ pendingOrder: order });
      } catch (e) {
        console.error("Failed to enqueue redownload:", e);
      }
    }
    info(`Download ${id} redownload requested (queued)`);
  },
  resumeDownload: async (id) => {
    let targetItem = get().downloads.find(d => d.id === id);
    if (!targetItem) return;
    
    set((state) => ({
      downloads: state.downloads.map(d => {
        if (d.id === id) {
          return { ...d, status: 'queued', speed: '-', eta: '-' };
        }
        return d;
      })
    }));

    try {
      const settings = useSettingsStore.getState();
      const login = getSiteLogin(targetItem.url, settings);
      let keychainPassword = null;
      if (login) {
        try {
          keychainPassword = await invoke('get_keychain_password', { id: login.id });
        } catch (e) {
          console.warn("Could not fetch keychain password for login:", e);
        }
      }
      const destPath = targetItem.destination || 
                       (settings.downloadDirectories && settings.downloadDirectories[targetItem.category]) || 
                       settings.defaultDownloadPath || 
                       '~/Downloads';
      const enqueueItem = {
        id: targetItem.id,
        url: targetItem.url,
        destination: destPath,
        filename: targetItem.fileName,
        connections: targetItem.connections || settings.perServerConnections || null,
        speed_limit: targetItem.speedLimit || settings.globalSpeedLimit || null,
        username: targetItem.username || (login ? login.username : null),
        password: targetItem.password || keychainPassword,
        headers: targetItem.headers || null,
        checksum: targetItem.checksum || null,
        cookies: targetItem.cookies || null,
        mirrors: targetItem.mirrors || null,
        user_agent: settings.customUserAgent || null,
        max_tries: settings.maxAutomaticRetries,
        proxy: await getProxyArgs(settings),
        format_selector: targetItem.mediaFormatSelector || null,
        cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
        is_media: targetItem.isMedia || false
      };
      await invoke('enqueue_download', { item: enqueueItem });
      const order = await invoke('get_pending_order');
      set({ pendingOrder: order });
    } catch (e) {
      console.error("Failed to enqueue resume:", e);
    }
  },
  startQueue: async (queueId) => {
    const runnable = get().downloads
      .filter(item => item.queueId === queueId && (item.status === 'queued' || item.status === 'paused' || item.status === 'failed'));

    if (runnable.length === 0) return 0;

    set((state) => ({
      downloads: state.downloads.map(item =>
        runnable.some(r => r.id === item.id)
          ? { ...item, status: 'queued', speed: '-', eta: '-' }
          : item
      )
    }));

    try {
      const settings = useSettingsStore.getState();
      const itemsToEnqueue = [];
      for (const item of runnable) {
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
                         
        itemsToEnqueue.push({
          id: item.id,
          url: item.url,
          destination: destPath,
          filename: item.fileName,
          connections: item.connections || settings.perServerConnections || null,
          speed_limit: item.speedLimit || settings.globalSpeedLimit || null,
          username: item.username || (login ? login.username : null),
          password: item.password || keychainPassword,
          headers: item.headers || null,
          checksum: item.checksum || null,
          cookies: item.cookies || null,
          mirrors: item.mirrors || null,
          user_agent: settings.customUserAgent || null,
          max_tries: settings.maxAutomaticRetries,
          proxy: await getProxyArgs(settings),
          format_selector: item.mediaFormatSelector || null,
          cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
          is_media: item.isMedia || false
        });
      }
      await invoke('enqueue_many', { items: itemsToEnqueue });
      const order = await invoke('get_pending_order');
      set({ pendingOrder: order });
    } catch (e) {
      console.error("Failed to start queue:", e);
    }

    info(`Queue ${queueId} started, ${runnable.length} items queued`);
    return runnable.length;
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
      
      // Reset interrupted downloads (crashed while downloading/processing) to queued
      set((state) => ({
        downloads: state.downloads.map(d =>
          d.status === 'downloading' || d.status === 'processing'
            ? { ...d, status: 'queued' as const }
            : d
        )
      }));

      // Auto resume downloads that were active or queued
      const active = get().downloads.filter(d => d.status === 'queued');
      if (active.length > 0) {
        try {
          const settings = useSettingsStore.getState();
          const itemsToEnqueue = [];
          for (const item of active) {
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
            itemsToEnqueue.push({
              id: item.id,
              url: item.url,
              destination: destPath,
              filename: item.fileName,
              connections: item.connections || settings.perServerConnections || null,
              speed_limit: item.speedLimit || settings.globalSpeedLimit || null,
              username: item.username || (login ? login.username : null),
              password: item.password || keychainPassword,
              headers: item.headers || null,
              checksum: item.checksum || null,
              cookies: item.cookies || null,
              mirrors: item.mirrors || null,
              user_agent: settings.customUserAgent || null,
              max_tries: settings.maxAutomaticRetries,
              proxy: await getProxyArgs(settings),
              format_selector: item.mediaFormatSelector || null,
              cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
              is_media: item.isMedia || false
            });
          }
          await invoke('enqueue_many', { items: itemsToEnqueue });
          const order = await invoke('get_pending_order');
          set({ pendingOrder: order });
        } catch (e) {
          console.error("Failed to auto-resume active downloads:", e);
        }
      }
    } catch (e) {
      console.error("Failed to init DB", e);
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
