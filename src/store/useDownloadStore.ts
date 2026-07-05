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

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

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
        } catch (e) {
          console.warn("Failed to retrieve keychain password for dispatch:", e);
        }
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
        user_agent: settings.customUserAgent.trim() || null,
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
      useDownloadStore.getState().updateDownload(id, { lastError: undefined });
      return true;
    } catch (e) {
      console.error(`Failed to dispatch ${id}:`, e);
      useDownloadStore.getState().updateDownload(id, {
        status: 'failed',
        lastError: errorMessage(e)
      });
      return false;
    } finally {
      backendDispatchPromises.delete(id);
    }
  })();

  backendDispatchPromises.set(id, promise);
  return promise;
}

export const normalizeCustomProxy = (host: string, port: number): string | null => {
  const trimmedHost = host.trim();
  const normalizedPort = Number.isFinite(port) ? Math.trunc(port) : NaN;
  if (!trimmedHost || !Number.isFinite(normalizedPort) || normalizedPort < 1 || normalizedPort > 65535) return null;

  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmedHost)) {
    try {
      const parsed = new URL(trimmedHost);
      if (!parsed.port) parsed.port = String(normalizedPort);
      return parsed.toString().replace(/\/$/, '');
    } catch {
      return null;
    }
  }

  return `http://${trimmedHost}:${normalizedPort}`;
};

export const getProxyArgs = async (settings: ReturnType<typeof useSettingsStore.getState>) => {
  if (settings.proxyMode === 'system') {
    try {
      const sysProxy = await invoke('get_system_proxy');
      return typeof sysProxy === 'string' && sysProxy ? sysProxy : "none";
    } catch (e) {
      console.warn("Failed to get system proxy:", e);
      return "none";
    }
  }
  if (settings.proxyMode === 'custom') {
    return normalizeCustomProxy(settings.proxyHost, settings.proxyPort) ?? "none";
  }
  if (settings.proxyMode === 'none') {
    return "none";
  }
  return null;
};

const escapeRegex = (value: string): string =>
  value.replace(/[.+?^${}()|[\]\\]/g, '\\$&');

const wildcardToRegex = (pattern: string): RegExp =>
  new RegExp(`^${escapeRegex(pattern).replace(/\*+/g, '.*')}$`);

const patternSpecificity = (pattern: string): number =>
  pattern.replace(/\*/g, '').length;

const hostPatternScore = (pattern: string, host: string): number | null => {
  if (pattern.startsWith('*.')) {
    const suffix = pattern.substring(2);
    return host === suffix || host.endsWith(`.${suffix}`)
      ? 1000 + patternSpecificity(pattern)
      : null;
  }

  if (pattern.includes('*')) {
    return wildcardToRegex(pattern).test(host)
      ? 1000 + patternSpecificity(pattern)
      : null;
  }

  return host === pattern ? 2000 + patternSpecificity(pattern) : null;
};

const urlPatternScore = (pattern: string, url: URL): number | null => {
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(pattern)) {
    const normalizedPattern = pattern.toLowerCase().replace(/\/+$/, '');
    const normalizedUrl = url.toString().toLowerCase().replace(/\/+$/, '');
    return wildcardToRegex(normalizedPattern).test(normalizedUrl)
      ? 4000 + patternSpecificity(normalizedPattern)
      : null;
  }

  if (pattern.includes('/')) {
    const normalizedPattern = pattern.toLowerCase().replace(/^\/+/, '');
    const normalizedTarget = `${url.hostname}${url.pathname}`.toLowerCase().replace(/\/+$/, '');
    return wildcardToRegex(normalizedPattern).test(normalizedTarget)
      ? 3000 + patternSpecificity(normalizedPattern)
      : null;
  }

  return hostPatternScore(pattern, url.hostname.toLowerCase());
};

export const getSiteLogin = (url: string, settings: ReturnType<typeof useSettingsStore.getState>) => {
  try {
    const urlObj = new URL(url);
    let bestMatch: { login: typeof settings.siteLogins[number]; score: number } | null = null;
    for (const login of settings.siteLogins) {
      const pattern = login.urlPattern.toLowerCase().trim();
      const score = pattern ? urlPatternScore(pattern, urlObj) : null;
      if (score !== null && (!bestMatch || score > bestMatch.score)) {
        bestMatch = { login, score };
      }
    }
    return bestMatch?.login ?? null;
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
  moveInQueue: (ids: string | string[], direction: 'up' | 'down') => Promise<void>;
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
  handleExtensionDownload: (request: ExtensionDownloadRequest) => Promise<void>;
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
  moveInQueue: async (idOrIds, direction) => {
    const ids = Array.isArray(idOrIds) ? idOrIds : [idOrIds];
    if (ids.length === 0) return;
    
    // Assume all items belong to the same queue as the first item
    const firstItem = get().downloads.find(d => d.id === ids[0]);
    if (!firstItem) return;
    const queueId = firstItem.queueId || MAIN_QUEUE_ID;
    
    const queueItems = get().downloads
      .filter(download => 
        (download.queueId || MAIN_QUEUE_ID) === queueId && 
        download.status !== 'completed' &&
        !(isActiveDownloadStatus(download.status) && download.status !== 'queued')
      )
      .sort((left, right) => (left.queuePosition ?? 0) - (right.queuePosition ?? 0));
    
    const selectedItems = queueItems.filter(item => ids.includes(item.id));
    if (selectedItems.length === 0) return;
    
    const unselectedItems = queueItems.filter(item => !ids.includes(item.id));
    const selectedIndices = selectedItems.map(item => queueItems.findIndex(d => d.id === item.id));
    
    let insertIndex = 0;
    if (direction === 'up') {
      const firstSelectedIndex = Math.min(...selectedIndices);
      insertIndex = Math.max(0, firstSelectedIndex - 1);
    } else {
      const lastSelectedIndex = Math.max(...selectedIndices);
      insertIndex = Math.min(unselectedItems.length, lastSelectedIndex - selectedItems.length + 2);
    }
    
    const reordered = [
      ...unselectedItems.slice(0, insertIndex),
      ...selectedItems,
      ...unselectedItems.slice(insertIndex)
    ];
    
    const positions = new Map(reordered.map((download, position) => [download.id, position]));
    const previousDownloads = get().downloads;
    set(state => ({
      downloads: state.downloads.map(download => positions.has(download.id)
        ? { ...download, queuePosition: positions.get(download.id) }
        : download)
    }));

    const registeredIdsToMove = selectedItems
      .filter(item => get().backendRegisteredIds.has(item.id))
      .map(item => item.id);
      
    if (registeredIdsToMove.length === 0) return;
    
    // For backend sync, we must call move_in_queue in the correct order to maintain the block
    const idsToMove = direction === 'up' ? registeredIdsToMove : [...registeredIdsToMove].reverse();

    try {
      let order: string[] = [];
      for (const id of idsToMove) {
        order = (await invoke('move_in_queue', { id, queueId, direction })) as string[];
      }
      if (order.length > 0) {
        set({ pendingOrder: order });
      }
    } catch (e) {
      console.error("Failed to move in queue backend:", e);
      set({ downloads: previousDownloads });
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
      pendingAddReferer: referer?.trim() || '',
      pendingAddFilename: filename?.trim() || '',
      pendingAddHeaders: headers?.trim() || '',
      pendingAddCookies: cookies?.trim() || ''
    };
  }),
  handleExtensionDownload: async (request) => {
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
    const queueItems = get().downloads.filter(download =>
      (download.queueId || MAIN_QUEUE_ID) === queueId
    );
    const maxPos = queueItems.reduce((max, d) => Math.max(max, d.queuePosition ?? 0), -1);
    const queuePosition = maxPos + 1;
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
    if (!url) throw new Error('Cannot redownload: original URL is missing.');

    // Remove from backend to clear its state and delete the existing file so we can overwrite
    try {
      await invoke('remove_download', { id, deleteAssets: true });
      get().unregisterBackendIds([id]);
    } catch (e) {
      console.warn("Could not remove old download from backend", e);
    }

    get().updateDownload(id, {
      status: 'queued',
      fraction: 0,
      speed: '-',
      eta: '-',
      hasBeenDispatched: false,
      dateAdded: new Date().toISOString()
    });

    if (!await dispatchItem(id)) {
      console.error("Failed to enqueue redownload");
      get().updateDownload(id, { status: 'failed' });
    } else {
      get().updateDownload(id, { hasBeenDispatched: true });
      info(`Download ${id} redownloaded (queued)`);
    }
  },
  resumeDownload: async (id) => {
    const targetItem = get().downloads.find(d => d.id === id);
    if (!targetItem) return false;

    try {
      if (targetItem.status === 'ready' || targetItem.status === 'staged') {
        get().updateDownload(id, { status: 'queued', hasBeenDispatched: true });
        if (await dispatchItem(id)) {
          return true;
        }
        get().updateDownload(id, { status: targetItem.status });
        return false;
      }

      const prevStatus = targetItem.status;
      const queueItems = get().downloads.filter(d => 
        (d.queueId || MAIN_QUEUE_ID) === (targetItem.queueId || MAIN_QUEUE_ID)
      );
      const maxPos = queueItems.reduce((max, d) => Math.max(max, d.queuePosition ?? 0), -1);
      
      get().updateDownload(id, { 
        status: 'queued', 
        speed: '-', 
        eta: '-',
        queuePosition: maxPos + 1
      });

      const resumedExisting = await invoke('resume_download', { id }).catch(() => false);
      
      let dispatchSucceeded = resumedExisting;
      if (!dispatchSucceeded) {
        get().unregisterBackendIds([id]);
        dispatchSucceeded = await dispatchItem(id);
      }

      if (dispatchSucceeded) {
        return true;
      } else {
        console.error("Failed to re-enqueue for resume");
        get().updateDownload(id, { status: prevStatus });
        return false;
      }
    } catch (e) {
      console.error("Failed to resume download:", e);
      get().updateDownload(id, { status: targetItem.status });
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
      const backendRegistered = get().backendRegisteredIds.has(item.id);
      const backendPending = get().pendingOrder.includes(item.id);

      if (item.status === 'queued' && backendRegistered && !backendPending) {
        if (await get().resumeDownload(item.id)) {
          acceptedIds.push(item.id);
        }
        continue;
      }

      if (
        item.status === 'ready' ||
        item.status === 'staged' ||
        item.status === 'failed' ||
        !item.hasBeenDispatched ||
        !backendRegistered
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

    const queueItems = get().downloads.filter(item =>
      !selectedIds.has(item.id) &&
      (item.queueId || MAIN_QUEUE_ID) === queueId
    );
    const maxPos = queueItems.reduce((max, d) => Math.max(max, d.queuePosition ?? 0), -1);
    const nextPosition = maxPos + 1;
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
      const active = get().downloads
        .filter(d => d.status === 'queued')
        .sort((a, b) => (a.queuePosition ?? 0) - (b.queuePosition ?? 0));
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
              user_agent: settings.customUserAgent.trim() || null,
              max_tries: settings.maxAutomaticRetries,
              proxy: await getProxyArgs(settings),
              format_selector: item.mediaFormatSelector || null,
              cookie_source: settings.mediaCookieSource !== 'none' ? settings.mediaCookieSource : null,
              is_media: item.isMedia || false
            });
          }
          const results = await invoke('enqueue_many', { items: itemsToEnqueue });
          const registeredIds = results.filter(result => result.success).map(result => result.id);
          const failedErrors = new Map(
            results
              .filter(result => !result.success)
              .map(result => [result.id, result.error || 'Backend rejected the queued download.'])
          );
          const order = await invoke('get_pending_order', { queueId: null });
          set(state => ({
            pendingOrder: order,
            backendRegisteredIds: new Set([
              ...state.backendRegisteredIds,
              ...registeredIds
            ]),
            downloads: state.downloads.map(download =>
              failedErrors.has(download.id)
                ? {
                    ...download,
                    status: 'failed' as const,
                    lastError: failedErrors.get(download.id)
                  }
                : registeredIds.includes(download.id)
                  ? { ...download, hasBeenDispatched: true, lastError: undefined }
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
let isSavingDownloads = false;
let nextDownloadsData: string | null = null;

async function processDownloadsSave() {
  if (isSavingDownloads || !nextDownloadsData) return;
  isSavingDownloads = true;
  while (nextDownloadsData) {
    const data = nextDownloadsData;
    nextDownloadsData = null;
    try {
      await invoke('db_replace_downloads', { data });
    } catch (error) {
      console.error('Failed to persist downloads:', error);
    }
  }
  isSavingDownloads = false;
}

let lastSavedQueues = '';
let isSavingQueues = false;
let nextQueuesData: string | null = null;

async function processQueuesSave() {
  if (isSavingQueues || !nextQueuesData) return;
  isSavingQueues = true;
  while (nextQueuesData) {
    const data = nextQueuesData;
    nextQueuesData = null;
    try {
      await invoke('db_replace_queues', { data });
    } catch (error) {
      console.error('Failed to persist queues:', error);
    }
  }
  isSavingQueues = false;
}

useDownloadStore.subscribe((state, prevState) => {
  if (state.queues !== prevState.queues) {
    const data = JSON.stringify(state.queues);
    if (data !== lastSavedQueues) {
      lastSavedQueues = data;
      nextQueuesData = data;
      processQueuesSave();
    }
  }

  if (state.downloads !== prevState.downloads) {
    // Strip secret fields (password/cookies/headers) and volatile progress
    // before writing to disk. Secrets remain on the in-memory item for the
    // active session only.
    const staticDownloads = state.downloads.map(redactDownloadForPersistence);
    
    const currentSerialized = JSON.stringify(staticDownloads);
    if (currentSerialized !== lastSavedDownloads) {
      lastSavedDownloads = currentSerialized;
      nextDownloadsData = currentSerialized;
      processDownloadsSave();
    }
  }
});
