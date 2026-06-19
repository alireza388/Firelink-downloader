import { create } from 'zustand';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgressEvent } from '../bindings/DownloadProgressEvent';
import type { DownloadStateEvent } from '../bindings/DownloadStateEvent';
import type { DownloadStatus } from '../bindings/DownloadStatus';

interface DownloadProgressState {
  progressMap: Record<string, DownloadProgressEvent>;
  updateDownloadProgress: (id: string, payload: DownloadProgressEvent) => void;
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
}));

let unlistenProgress: UnlistenFn | null = null;
let unlistenState: UnlistenFn | null = null;
let unlistenTray: UnlistenFn | null = null;

export async function initDownloadListener() {
  if (unlistenProgress) return;
  unlistenProgress = await listen<DownloadProgressEvent>('download-progress', (event) => {
    const payload = event.payload;
    useDownloadProgressStore.getState().updateDownloadProgress(payload.id, payload);

    const mainStore = useDownloadStore.getState();
    const current = mainStore.downloads.find(d => d.id === payload.id);
    if (current) {
      mainStore.updateDownload(payload.id, {
        fraction: payload.fraction,
        speed: payload.speed,
        eta: payload.eta,
        ...(payload.size ? { size: payload.size } : {}),
      });
    }
  });

  if (!unlistenState) {
    unlistenState = await listen<DownloadStateEvent>('download-state', (event) => {
      const payload = event.payload;
      const mainStore = useDownloadStore.getState();
      const current = mainStore.downloads.find(d => d.id === payload.id);
      if (current) {
        const status = payload.status as DownloadStatus;
        const updates: Partial<any> = { status };
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
      }
    });
  }

  if (!unlistenTray) {
    unlistenTray = await listen<string>('tray-action', (event) => {
      const mainStore = useDownloadStore.getState();
      if (event.payload === 'pause-all') {
        const uniqueQueues = Array.from(new Set(mainStore.downloads.map(d => d.queueId)));
        uniqueQueues.forEach(qid => mainStore.pauseQueue(qid));
      } else if (event.payload === 'resume-all') {
        const uniqueQueues = Array.from(new Set(mainStore.downloads.map(d => d.queueId)));
        uniqueQueues.forEach(qid => mainStore.startQueue(qid));
      }
    });
  }
}
