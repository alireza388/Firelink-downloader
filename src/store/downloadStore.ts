import { create } from 'zustand';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgressEvent } from '../bindings/DownloadProgressEvent';
import type { DownloadStatus } from '../bindings/DownloadStatus';
import { listenEvent as listen } from '../ipc';
import type { DownloadItem } from '../bindings/DownloadItem';
import { categoryForFileName } from '../utils/downloads';

interface DownloadProgressState {
  progressMap: Record<string, DownloadProgressEvent>;
  updateDownloadProgress: (id: string, payload: DownloadProgressEvent) => void;
  clearDownloadProgress: (id: string) => void;
}

import { useDownloadStore } from './useDownloadStore';

export const useDownloadProgressStore = create<DownloadProgressState>((set) => ({
  progressMap: {},
  updateDownloadProgress: (id, payload) =>
    set((state) => ({
      progressMap: {
        ...state.progressMap,
        [id]: payload,
      },
    })),
  clearDownloadProgress: (id) =>
    set((state) => {
      if (!(id in state.progressMap)) return state;
      const next = { ...state.progressMap };
      delete next[id];
      return { progressMap: next };
    }),
}));

let unlistenProgress: UnlistenFn | null = null;
let unlistenState: UnlistenFn | null = null;
let unlistenTray: UnlistenFn | null = null;
let listenerSetup: Promise<void> | null = null;
let listenerConsumers = 0;

const disposeDownloadListeners = () => {
  unlistenProgress?.();
  unlistenProgress = null;
  unlistenState?.();
  unlistenState = null;
  unlistenTray?.();
  unlistenTray = null;
  listenerSetup = null;
};

const startDownloadListeners = async () => {
  const registrations = await Promise.allSettled([
    listen('download-progress', (event) => {
      const payload = event.payload;
      useDownloadProgressStore.getState().updateDownloadProgress(payload.id, payload);

      const mainStore = useDownloadStore.getState();
      const current = mainStore.downloads.find(d => d.id === payload.id);
      if (current) {
        const shouldUpdateSize = Boolean(payload.size && (!current.isMedia || payload.size_is_final));
        const updates: Partial<DownloadItem> = {};
        if (current.status === 'downloading' || current.status === 'processing') {
          updates.fraction = payload.fraction;
          updates.speed = payload.speed;
          updates.eta = payload.eta;
        }
        if (shouldUpdateSize && current.size !== payload.size) {
          updates.size = payload.size!;
        }
        if (Object.keys(updates).length > 0) {
          mainStore.updateDownload(payload.id, updates);
        }
      }
    }),
    listen('download-state', (event) => {
      const payload = event.payload;
      const mainStore = useDownloadStore.getState();
      const current = mainStore.downloads.find(d => d.id === payload.id);
      if (current) {
        const status = payload.status as DownloadStatus;

        // Prevent race condition: don't transition backwards from terminal state
        if ((current.status === 'completed' || current.status === 'failed') &&
            (status !== 'completed' && status !== 'failed')) {
          return;
        }

        const progress = useDownloadProgressStore.getState().progressMap[payload.id];
        const updates: Partial<DownloadItem> = {
          status,
          ...(progress ? { fraction: progress.fraction } : {}),
          ...(payload.error ? { lastError: payload.error } : {}),
          ...((status === 'downloading' || status === 'retrying')
            ? { lastTry: new Date().toISOString() }
            : {})
        };
        if (!payload.error && status !== 'failed' && status !== 'retrying') {
          updates.lastError = undefined;
        }
        if (payload.fileName && payload.fileName !== current.fileName) {
          updates.fileName = payload.fileName;
          updates.category = categoryForFileName(payload.fileName);
        }
        if (status !== 'downloading') {
          updates.speed = '-';
          updates.eta = '-';
        }
        mainStore.updateDownload(payload.id, updates);

        if (status === 'completed' || status === 'failed' || status === 'paused') {
          mainStore.setPendingOrder(mainStore.pendingOrder.filter(id => id !== payload.id));
        } else if (status === 'queued') {
          if (!mainStore.pendingOrder.includes(payload.id)) {
            mainStore.setPendingOrder([...mainStore.pendingOrder, payload.id]);
          }
        }

        if (status === 'queued' || status === 'downloading' || status === 'processing' || status === 'retrying') {
          mainStore.registerBackendIds([payload.id]);
        } else if (status === 'completed' || status === 'failed') {
          mainStore.unregisterBackendIds([payload.id]);
        }
        if (status === 'completed' || status === 'failed' || status === 'paused') {
          useDownloadProgressStore.getState().clearDownloadProgress(payload.id);
        }
      }
    }),
    listen('tray-action', (event) => {
      const mainStore = useDownloadStore.getState();
      if (event.payload === 'pause-all') {
        void mainStore.pauseAll();
      } else if (event.payload === 'resume-all') {
        void mainStore.startAll();
      }
    }),
  ]);

  const failedRegistration = registrations.find(
    (registration): registration is PromiseRejectedResult => registration.status === 'rejected'
  );
  if (failedRegistration) {
    for (const registration of registrations) {
      if (registration.status === 'fulfilled') registration.value();
    }
    throw failedRegistration.reason;
  }

  const [progress, state, tray] = registrations as [
    PromiseFulfilledResult<UnlistenFn>,
    PromiseFulfilledResult<UnlistenFn>,
    PromiseFulfilledResult<UnlistenFn>,
  ];
  unlistenProgress = progress.value;
  unlistenState = state.value;
  unlistenTray = tray.value;
};

export async function initDownloadListener(): Promise<() => void> {
  listenerConsumers += 1;
  if (!listenerSetup) {
    listenerSetup = startDownloadListeners().catch(error => {
      disposeDownloadListeners();
      throw error;
    });
  }

  try {
    await listenerSetup;
  } catch (error) {
    listenerConsumers -= 1;
    throw error;
  }

  let released = false;
  return () => {
    if (released) return;
    released = true;
    listenerConsumers -= 1;
    if (listenerConsumers === 0) disposeDownloadListeners();
  };
}
