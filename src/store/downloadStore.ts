import { create } from 'zustand';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgressEvent } from '../bindings/DownloadProgressEvent';

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
let unlistenTray: UnlistenFn | null = null;

export async function initDownloadListener() {
  if (unlistenProgress) return;
  unlistenProgress = await listen<DownloadProgressEvent>('download-progress', (event) => {
    const payload = event.payload;
    useDownloadProgressStore.getState().updateDownloadProgress(payload.id, payload);

    const mainStore = useDownloadStore.getState();
    const current = mainStore.downloads.find(d => d.id === payload.id);
    if (current && current.status === 'queued') {
      const updates: any = { status: 'downloading' };
      if (payload.size && current.size !== payload.size) updates.size = payload.size;
      mainStore.updateDownload(payload.id, updates);
    } else if (current && payload.size && current.size !== payload.size) {
      mainStore.updateDownload(payload.id, { size: payload.size });
    }
  });

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
