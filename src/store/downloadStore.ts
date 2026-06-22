import { create } from 'zustand';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgressEvent } from '../bindings/DownloadProgressEvent';
import type { DownloadStatus } from '../bindings/DownloadStatus';
import { listenEvent as listen } from '../ipc';

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

export async function initDownloadListener() {
  if (unlistenProgress) return;
  unlistenProgress = await listen('download-progress', (event) => {
    const payload = event.payload;
    useDownloadProgressStore.getState().updateDownloadProgress(payload.id, payload);

    const mainStore = useDownloadStore.getState();
    const current = mainStore.downloads.find(d => d.id === payload.id);
    if (current) {
      const shouldUpdateSize = Boolean(payload.size && (!current.isMedia || payload.size_is_final));
      if (shouldUpdateSize && current.size !== payload.size) {
        mainStore.updateDownload(payload.id, { size: payload.size! });
      }
    }
  });

  if (!unlistenState) {
    unlistenState = await listen('download-state', (event) => {
      const payload = event.payload;
      const mainStore = useDownloadStore.getState();
      const current = mainStore.downloads.find(d => d.id === payload.id);
      if (current) {
        const status = payload.status as DownloadStatus;
        const progress = useDownloadProgressStore.getState().progressMap[payload.id];
        const updates: Partial<any> = {
          status,
          ...(progress ? { fraction: progress.fraction } : {})
        };
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
    });
  }

  if (!unlistenTray) {
    unlistenTray = await listen('tray-action', (event) => {
      const mainStore = useDownloadStore.getState();
      if (event.payload === 'pause-all') {
        void mainStore.pauseAll();
      } else if (event.payload === 'resume-all') {
        void mainStore.startAll();
      }
    });
  }
}
