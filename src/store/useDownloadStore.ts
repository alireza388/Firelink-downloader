import { create } from 'zustand';
import { info } from '../utils/logger';
import { invokeCommand as invoke } from '../ipc';

import type { DownloadItem } from '../bindings/DownloadItem';
import type { DownloadStatus } from '../bindings/DownloadStatus';
import type { ExtensionDownload } from '../bindings/ExtensionDownload';
import type { Queue } from '../bindings/Queue';
import { useSettingsStore } from './useSettingsStore';
import { categoryForFileName, isActiveDownloadStatus, normalizeSpeedLimitForBackend, redactDownloadForPersistence } from '../utils/downloads';
import {
  resolveCategoryDestination
} from '../utils/downloadLocations';
import { canPauseDownload, canStartDownload } from '../utils/downloadActions';

export type { DownloadCategory } from '../utils/downloads';

const backendDispatchPromises = new Map<string, Promise<boolean>>();

export async function dispatchItem(id: string): Promise<boolean> {
  if (backendDispatchPromises.has(id)) return backendDispatchPromises.get(id)!;

  const promise = (async () => {
    try {
      const state = useDownloadStore.getState();
      const item = state.downloads.find(d => d.id === id);
      if (!item) return false;
      if (state.backendRegisteredIds.has(id)) return true;

      const settings = useSettingsStore.getState();
      const destination = item.destination ||
        await resolveCategoryDestination(settings, item.category);
      const login = getSiteLogin(item.url, settings);
      let keychainPassword = null;
      if (login) {
        try {
          keychainPassword = await invoke('get_keychain_password', { id: login.id });
        } catch (e) {}
      }

      const enqueueItem = {
        id: item.id,
        queue_id: item.queueId || MAIN_QUEUE_ID,
        url: item.url,
        destination,
        filename: item.fileName,
        connections: item.connections || settings.perServerConnections || null,
        speed_limit: item.speedLimit || normalizeSpeedLimitForBackend(settings.globalSpeedLimit),
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

      const accepted = await invoke('enqueue_download', { item: enqueueItem });
      const acceptedFilename = accepted?.filename || item.fileName;
      if (acceptedFilename !== item.fileName) {
        useDownloadStore.getState().updateDownload(id, {
          fileName: acceptedFilename,
          category: categoryForFileName(acceptedFilename)
        });
      }
      const order = await invoke('get_pending_order', { queueId: item.queueId || MAIN_QUEUE_ID });
      useDownloadStore.getState().setPendingOrder(order);
      useDownloadStore.getState().registerBackendIds([id]);
      return true;
    } catch (e) {
      console.error(`Failed to dispatch ${id}:`, e);
      useDownloadStore.getState().updateDownload(id, { status: 'failed' });
      return false;
    } finally {
      backendDispatchPromises.delete(id);
    }
  })();

  backendDispatchPromises.set(id, promise);
  return promise;
}

const getProxyArgs = async (settings: ReturnType<typeof useSettingsStore.getState>) => {
  if (settings.proxyMode === 'system') {
    try {
      const sysProxy = await invoke('get_system_proxy');
      return typeof sysProxy === 'string' && sysProxy ? sysProxy : "none";
    } catch (e) {
      console.warn("Failed to get system proxy:", e);
      return "none";
    }
  }
  if (settings.proxyMode === 'custom' && settings.proxyHost) {
    return `http://${settings.proxyHost}:${settings.proxyPort}`;
  }
  if (settings.proxyMode === 'none') {
    return "none";
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
};

const effectiveDestinationForItem = async (
  item: Pick<DownloadItem, 'destination' | 'category'>,
  settings: ReturnType<typeof useSettingsStore.getState>
): Promise<string> =>
  item.destination || resolveCategoryDestination(settings, item.category);

const normalizeQueuePositions = (downloads: DownloadItem[]): DownloadItem[] => {
  const nextPosition = new Map<string, number>();
  return downloads.map(download => {
    const queueId = download.queueId || MAIN_QUEUE_ID;
    const position = nextPosition.get(queueId) || 0;
    nextPosition.set(queueId, position + 1);
    return {
      ...download,
      queueId,
      queuePosition: download.queuePosition ?? position
    };
  });
};

export type { DownloadStatus };
export const MAIN_QUEUE_ID = '00000000-0000-0000-0000-000000000001';

export type { DownloadItem, Queue };
export type ExtensionDownloadRequest = ExtensionDownload;
export type AddDownloadAction =
  | { type: 'start-now' }
  | { type: 'add-to-queue'; queueId: string };
export type DownloadDraft = Omit<DownloadItem, 'status' | 'queueId' | 'hasBeenDispatched'>;

export type DeleteModalState = {
  isOpen: boolean;
  downloadIds?: string[];
};

interface DownloadState {
  downloads: DownloadItem[];
  queues: Queue[];
  pendingOrder: string[];
  setPendingOrder: (order: string[]) => void;
  backendRegisteredIds: Set<string>;
  registerBackendIds: (ids: string[]) => void;
  unregisterBackendIds: (ids: string[]) => void;
  applyProperties: (id: string, updates: Partial<DownloadItem>) => Promise<void>;
  moveInQueue: (id: string, direction: 'up' | 'down') => Promise<void>;
  removeFromQueue: (id: string) => Promise<void>;
  isAddModalOpen: boolean;
  pendingAddUrls: string;
  pendingAddReferer: string;
  pendingAddFilename: string;
  pendingAddHeaders: string;
  pendingAddCookies: string;
  selectedPropertiesDownloadId: string | null;
  toggleAddModal: (isOpen: boolean) => void;
  openAddModalWithUrls: (
    urls: string,
    referer?: string | null,
    filename?: string | null,
    headers?: string | null,
    cookies?: string | null
  ) => void;
  handleExtensionDownload: (request: ExtensionDownloadRequest) => void;
  deleteModalState: DeleteModalState;
  openDeleteModal: (downloadIds?: string | string[]) => void;
  closeDeleteModal: () => void;
  setSelectedPropertiesDownloadId: (id: string | null) => void;
  addDownload: (item: DownloadDraft, action: AddDownloadAction) => Promise<boolean>;
  updateDownload: (id: string, updates: Partial<DownloadItem>) => void;
  removeDownload: (id: string, deleteFile?: boolean) => Promise<void>;
  redownload: (id: string) => Promise<void>;
  resumeDownload: (id: string) => Promise<boolean>;
  startQueue: (queueId: string) => Promise<string[]>;
  pauseQueue: (queueId: string) => Promise<number>;
  startAll: () => Promise<number>;
  pauseAll: () => Promise<number>;
  assignToQueue: (ids: string[], queueId: string) => Promise<void>;
  addQueue: (name: string) => void;
  renameQueue: (id: string, name: string) => void;
  removeQueue: (id: string) => Promise<void>;
  initDB: () => Promise<void>;
  
}

export const useDownloadStore = create<DownloadState>((set, get) => ({
  downloads: [],
  queues: [{ id: MAIN_QUEUE_ID, name: 'Main Queue', isMain: true }],
  pendingOrder: [],
  setPendingOrder: (order) => set({ pendingOrder: order }),
  moveInQueue: async (id, direction) => {
    const item = get().downloads.find(download => download.id === id);
    if (!item) return;
    const queueId = item.queueId || MAIN_QUEUE_ID;
    const queueItems = get().downloads
      .filter(download => (download.queueId || MAIN_QUEUE_ID) === queueId && download.status !== 'completed')
      .sort((left, right) => (left.queuePosition ?? 0) - (right.queuePosition ?? 0));
    const index = queueItems.findIndex(download => download.id === id);
    const target = direction === 'up' ? index - 1 : index + 1;
    if (index < 0 || target < 0 || target >= queueItems.length) return;
    const reordered = [...queueItems];
    [reordered[index], reordered[target]] = [reordered[target], reordered[index]];
    const positions = new Map(reordered.map((download, position) => [download.id, position]));
    set(state => ({
      downloads: state.downloads.map(download => positions.has(download.id)
        ? { ...download, queuePosition: positions.get(download.id) }
        : download)
    }));

    if (!get().backendRegisteredIds.has(id)) return;
    try {
      const order = await invoke('move_in_queue', { id, queueId, direction });
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
  backendRegisteredIds: new Set(),
  registerBackendIds: (ids) => set((state) => {
    const nextSet = new Set(state.backendRegisteredIds);
    for (const id of ids) nextSet.add(id);
    return { backendRegisteredIds: nextSet };
  }),
  unregisterBackendIds: (ids) => set((state) => {
    const nextSet = new Set(state.backendRegisteredIds);
    for (const id of ids) nextSet.delete(id);
    return { backendRegisteredIds: nextSet };
  }),
  isAddModalOpen: false,
  pendingAddUrls: '',
  pendingAddReferer: '',
  pendingAddFilename: '',
  pendingAddHeaders: '',
  pendingAddCookies: '',
  selectedPropertiesDownloadId: null,
  deleteModalState: { isOpen: false },
  openDeleteModal: (downloadIds) => set({ 
    deleteModalState: { 
      isOpen: true, 
      downloadIds: Array.isArray(downloadIds) ? downloadIds : (downloadIds ? [downloadIds] : undefined) 
    } 
  }),
  closeDeleteModal: () => set({ deleteModalState: { isOpen: false } }),
  toggleAddModal: (isOpen) => set({
    isAddModalOpen: isOpen,
    pendingAddUrls: '',
    pendingAddReferer: '',
    pendingAddFilename: '',
    pendingAddHeaders: '',
    pendingAddCookies: ''
  }),
  openAddModalWithUrls: (urls, referer, filename, headers, cookies) => set((state) => {
    const existingUrls = state.isAddModalOpen && state.pendingAddUrls ? state.pendingAddUrls : '';
    const mergedUrls = existingUrls ? `${existingUrls}\n${urls}` : urls;
    return {
      isAddModalOpen: true,
      pendingAddUrls: mergedUrls,
      pendingAddReferer: referer?.trim() || state.pendingAddReferer || '',
      pendingAddFilename: filename?.trim() || state.pendingAddFilename || '',
      pendingAddHeaders: headers?.trim() || state.pendingAddHeaders || '',
      pendingAddCookies: cookies?.trim() || state.pendingAddCookies || ''
    };
  }),
  handleExtensionDownload: (request) => {
    const urls = [...new Set(request.urls.map(url => url.trim()).filter(Boolean))];
    if (urls.length === 0) return;

    get().openAddModalWithUrls(
      urls.join('\n'),
      request.referer,
      urls.length === 1 ? request.filename : null,
      request.headers,
      request.cookies
    );
  },
  setSelectedPropertiesDownloadId: (id) => set({ selectedPropertiesDownloadId: id }),
  addDownload: async (item, action) => {
    const settings = useSettingsStore.getState();
    const destPath = await effectiveDestinationForItem(item, settings);
    const queueId = action.type === 'add-to-queue' ? action.queueId : MAIN_QUEUE_ID;
    const queuePosition = get().downloads.filter(download =>
      (download.queueId || MAIN_QUEUE_ID) === queueId && download.status !== 'completed'
    ).length;
    const ownedItem: DownloadItem = {
      ...item,
      destination: destPath,
      status: action.type === 'add-to-queue' ? 'staged' : 'ready',
      queueId,
      queuePosition,
      hasBeenDispatched: false
    };
    set((state) => ({ downloads: [...state.downloads, ownedItem] }));

    if (action.type === 'add-to-queue') {
      info(`Download ${item.id} added to queue ${action.queueId}`);
      return true;
    } else if (action.type === 'start-now') {
      if (await dispatchItem(item.id)) {
        get().updateDownload(item.id, { hasBeenDispatched: true });
        info(`Download ${item.id} started`);
        return true;
      }
      return false;
    }
    return false;
  },
  applyProperties: async (id, updates) => {
    const state = get();
    const item = state.downloads.find(d => d.id === id);
    if (!item) return;

    if (item.status === 'downloading' || item.status === 'processing' || item.status === 'retrying') {
      throw new Error("Cannot change properties while transfer is active. Pause it first.");
    }

    if (item.status === 'ready' || item.status === 'staged' || item.status === 'completed' || item.status === 'failed') {
      state.updateDownload(id, updates);
      return;
    }

    // Queued or Paused
    const isRegistered = state.backendRegisteredIds.has(id);

    if (item.status === 'queued') {
      if (isRegistered) {
        await invoke('remove_from_queue', { id });
        state.unregisterBackendIds([id]);
      }
      state.updateDownload(id, updates);
      if (isRegistered) {
        if (!await dispatchItem(id)) {
          state.removeFromQueue(id);
        }
      }
    } else if (item.status === 'paused') {
      if (isRegistered) {
        try {
          await invoke('detach_download_for_reconfigure', { id });
        } catch (e) {
          console.error("Failed to detach for reconfigure:", e);
          throw e; // Preserve old properties if detach fails
        }
        state.unregisterBackendIds([id]);
      }
      state.updateDownload(id, updates);
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
      await invoke('remove_download', { id, deleteAssets: deleteFile });
    }

    set((state) => ({
      downloads: state.downloads.filter(d => d.id !== id),
      pendingOrder: state.pendingOrder.filter(x => x !== id),
      backendRegisteredIds: new Set(
        Array.from(state.backendRegisteredIds).filter(registeredId => registeredId !== id)
      )
    }));
    info(`Download ${id} removed`);
    syncSystemIntegrations();
  },
  redownload: async (id) => {
    const targetItem = get().downloads.find(d => d.id === id);
    if (!targetItem) {
      throw new Error('Cannot redownload: download was not found.');
    }

    if (!['completed', 'failed', 'paused'].includes(targetItem.status)) {
      throw new Error(`Cannot redownload a ${targetItem.status} download. Pause or wait for it to finish first.`);
    }

    const url = targetItem.url?.trim();
    if (!url) {
      throw new Error('Cannot redownload: original URL is missing.');
    }

    const filename = targetItem.fileName?.trim();
    if (!filename) {
      throw new Error('Cannot redownload: original filename is missing.');
    }

    const mediaFormatSelector = targetItem.mediaFormatSelector?.trim();

    const settings = useSettingsStore.getState();
    const destPath = targetItem.destination ||
      await resolveCategoryDestination(settings, targetItem.category);

    if (!destPath.trim()) {
      throw new Error('Cannot redownload: destination folder is missing.');
    }

    const redownloadItem: DownloadItem = {
      id: crypto.randomUUID(),
      url,
      fileName: filename,
      status: 'queued',
      category: targetItem.category,
      dateAdded: new Date().toISOString(),
      connections: targetItem.connections,
      speedLimit: targetItem.speedLimit,
      username: targetItem.username,
      password: targetItem.password,
      headers: targetItem.headers,
      checksum: targetItem.checksum,
      cookies: targetItem.cookies,
      mirrors: targetItem.mirrors,
      destination: destPath,
      isMedia: targetItem.isMedia,
      mediaFormatSelector,
      queueId: targetItem.queueId || MAIN_QUEUE_ID,
      hasBeenDispatched: false
    };

    set((state) => ({
      downloads: [...state.downloads, redownloadItem]
    }));

    if (!await dispatchItem(redownloadItem.id)) {
      console.error("Failed to enqueue redownload");
    } else {
      info(`Download ${id} redownload requested as ${redownloadItem.id} (queued)`);
    }
  },
  resumeDownload: async (id) => {
    const targetItem = get().downloads.find(d => d.id === id);
    if (!targetItem) return false;

    try {
      if (targetItem.status === 'ready' || targetItem.status === 'staged') {
        if (await dispatchItem(id)) {
          get().updateDownload(id, { hasBeenDispatched: true });
          return true;
        }
        return false;
      }

      const resumedExisting = await invoke('resume_download', { id });
      if (resumedExisting) {
        return true;
      }

      get().unregisterBackendIds([id]);

      set((state) => ({
        downloads: state.downloads.map(d => {
          if (d.id === id) {
            return { ...d, status: 'queued', speed: '-', eta: '-' };
          }
          return d;
        })
      }));

      if (!await dispatchItem(id)) {
        console.error("Failed to re-enqueue for resume");
        return false;
      }
      return true;
    } catch (e) {
      console.error("Failed to resume download:", e);
      return false;
    }
  },
  startQueue: async (queueId) => {
    const runnable = get().downloads
      .filter(item => item.queueId === queueId && (item.status === 'queued' || canStartDownload(item.status)))
      .sort((left, right) => (left.queuePosition ?? 0) - (right.queuePosition ?? 0));

    if (runnable.length === 0) return [];

    const acceptedIds: string[] = [];
    for (const item of runnable) {
      if (
        item.status === 'ready' ||
        item.status === 'staged' ||
        item.status === 'failed' ||
        !item.hasBeenDispatched ||
        !get().backendRegisteredIds.has(item.id)
      ) {
        if (await dispatchItem(item.id)) {
          const current = get().downloads.find(download => download.id === item.id);
          get().updateDownload(item.id, {
            hasBeenDispatched: true,
            ...(current?.status === item.status ? { status: 'queued' as const } : {})
          });
          acceptedIds.push(item.id);
        }
      } else if (item.status === 'paused' || item.status === 'queued') {
        // If it's queued but already dispatched, it might be waiting. 
        // If it's paused, we resume it.
        if (item.status === 'paused') {
          if (!await get().resumeDownload(item.id)) continue;
        }
        acceptedIds.push(item.id);
      }
    }

    info(`Queue ${queueId} started, ${acceptedIds.length} items dispatched/resumed`);
    return acceptedIds;
  },
  pauseQueue: async (queueId) => {
    const activeIds = get().downloads
      .filter(item => item.queueId === queueId && canPauseDownload(item.status))
      .map(item => item.id);

    if (activeIds.length === 0) return 0;

    const results = await Promise.allSettled(
      activeIds.map(id => invoke('pause_download', { id }))
    );
    const pausedCount = results.filter(result => result.status === 'fulfilled').length;
    const failedCount = activeIds.length - pausedCount;
    if (failedCount > 0) {
      console.error(`Failed to pause ${failedCount} downloads in queue ${queueId}`);
    }
    info(`Queue ${queueId} paused, ${pausedCount} items paused`);
    syncSystemIntegrations();
    return pausedCount;
  },
  startAll: async () => {
    set(state => ({
      downloads: state.downloads.map(item =>
        item.queueId ? item : { ...item, queueId: MAIN_QUEUE_ID }
      )
    }));
    const queueIds = new Set(
      get().downloads
        .filter(item => item.status === 'queued' || canStartDownload(item.status))
        .map(item => item.queueId || MAIN_QUEUE_ID)
    );
    const results = await Promise.all(Array.from(queueIds, queueId => get().startQueue(queueId)));
    return results.reduce((total, ids) => total + ids.length, 0);
  },
  pauseAll: async () => {
    const activeIds = get().downloads
      .filter(item => canPauseDownload(item.status))
      .map(item => item.id);
    if (activeIds.length === 0) return 0;

    const results = await Promise.allSettled(
      activeIds.map(id => invoke('pause_download', { id }))
    );
    const pausedCount = results.filter(result => result.status === 'fulfilled').length;
    syncSystemIntegrations();
    return pausedCount;
  },
  assignToQueue: async (ids, queueId) => {
    const selectedIds = new Set(ids);
    const selected = get().downloads.filter(item => selectedIds.has(item.id));
    const locked = selected.find(item => isActiveDownloadStatus(item.status) && item.status !== 'queued');
    if (locked) {
      throw new Error(`Pause ${locked.fileName} before moving it to another queue.`);
    }

    for (const item of selected) {
      if (!get().backendRegisteredIds.has(item.id)) continue;
      if (item.status === 'queued') {
        await invoke('remove_from_queue', { id: item.id });
      } else if (item.status === 'paused') {
        await invoke('detach_download_for_reconfigure', { id: item.id });
      }
      get().unregisterBackendIds([item.id]);
    }

    const nextPosition = get().downloads.filter(item =>
      !selectedIds.has(item.id) &&
      (item.queueId || MAIN_QUEUE_ID) === queueId &&
      item.status !== 'completed'
    ).length;
    set(state => ({
      downloads: state.downloads.map(item =>
        selectedIds.has(item.id) && item.status !== 'completed'
          ? {
              ...item,
              queueId,
              queuePosition: nextPosition + selected.findIndex(selectedItem => selectedItem.id === item.id),
              status: 'staged' as const,
              hasBeenDispatched: false
            }
          : item
      )
    }));
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
  removeQueue: async (id) => {
    if (id === MAIN_QUEUE_ID) return;
    const unfinishedIds = get().downloads
      .filter(download => download.queueId === id && download.status !== 'completed')
      .map(download => download.id);
    if (unfinishedIds.length > 0) {
      await get().assignToQueue(unfinishedIds, MAIN_QUEUE_ID);
    }
    set((state) => ({
      queues: state.queues.filter(q => q.id !== id),
      downloads: state.downloads.map(d => 
        d.queueId === id ? { ...d, queueId: MAIN_QUEUE_ID } : d
      )
    }));
    const settings = useSettingsStore.getState();
    if (settings.scheduler.selectedQueueIds.includes(id)) {
      const selectedQueueIds = settings.scheduler.selectedQueueIds.filter(queueId => queueId !== id);
      settings.setScheduler({
        ...settings.scheduler,
        enabled: selectedQueueIds.length > 0 ? settings.scheduler.enabled : false,
        selectedQueueIds
      });
    }
  },
  initDB: async () => {
    try {
      const queues = (await invoke('db_get_all_queues')).map(value => JSON.parse(value) as Queue);
      const downloads = (await invoke('db_get_all_downloads')).map(
        value => JSON.parse(value) as DownloadItem
      );
      
      set(state => ({
        queues: queues.length > 0 ? queues : state.queues,
        downloads: downloads.length > 0
          ? normalizeQueuePositions(downloads)
          : state.downloads
      }));
      
      // Reset interrupted active downloads to queued.
      set((state) => ({
        downloads: state.downloads.map(d =>
          isActiveDownloadStatus(d.status) && d.status !== 'queued'
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
              await resolveCategoryDestination(settings, item.category);
            itemsToEnqueue.push({
              id: item.id,
              queue_id: item.queueId || MAIN_QUEUE_ID,
              url: item.url,
              destination: destPath,
              filename: item.fileName,
              connections: item.connections || settings.perServerConnections || null,
            speed_limit: item.speedLimit || normalizeSpeedLimitForBackend(settings.globalSpeedLimit),
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
          const results = await invoke('enqueue_many', { items: itemsToEnqueue });
          const registeredIds = results.filter(result => result.success).map(result => result.id);
          const failedIds = new Set(results.filter(result => !result.success).map(result => result.id));
          const order = await invoke('get_pending_order', { queueId: null });
          set(state => ({
            pendingOrder: order,
            backendRegisteredIds: new Set([
              ...state.backendRegisteredIds,
              ...registeredIds
            ]),
            downloads: state.downloads.map(download =>
              failedIds.has(download.id)
                ? { ...download, status: 'failed' as const }
                : registeredIds.includes(download.id)
                  ? { ...download, hasBeenDispatched: true }
                  : download
            )
          }));
        } catch (e) {
          console.error("Failed to auto-resume active downloads:", e);
        }
      }
    } catch (e) {
      console.error("Failed to init DB", e);
      throw e;
    }
  }
}));

let lastSavedDownloads = '';
let downloadsSave = Promise.resolve();
let queuesSave = Promise.resolve();

useDownloadStore.subscribe(async (state, prevState) => {
  if (state.queues !== prevState.queues) {
    const data = JSON.stringify(state.queues);
    queuesSave = queuesSave
      .then(() => invoke('db_replace_queues', { data }))
      .catch(error => {
        console.error('Failed to persist queues:', error);
      });
    await queuesSave;
  }

  if (state.downloads !== prevState.downloads) {
    // Strip secret fields (password/cookies/headers) and volatile progress
    // before writing to disk. Secrets remain on the in-memory item for the
    // active session only.
    const staticDownloads = state.downloads.map(redactDownloadForPersistence);
    
    const currentSerialized = JSON.stringify(staticDownloads);
    if (currentSerialized !== lastSavedDownloads) {
      lastSavedDownloads = currentSerialized;
      downloadsSave = downloadsSave
        .then(() => invoke('db_replace_downloads', { data: currentSerialized }))
        .catch(error => {
          console.error('Failed to persist downloads:', error);
        });
      await downloadsSave;
    }
  }
});
