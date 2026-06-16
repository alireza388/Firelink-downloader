import { create } from 'zustand';
import { persist, createJSONStorage, StateStorage } from 'zustand/middleware';
import { LazyStore } from '@tauri-apps/plugin-store';
import { info } from '@tauri-apps/plugin-log';

export const tauriStore = new LazyStore('store.bin');

const tauriStorage: StateStorage = {
  getItem: async (name: string): Promise<string | null> => {
    if (name === 'firelink-engine-settings') {
      try {
        const data = await tauriStore.get<string>('engine_settings');
        return data || null;
      } catch (e) {
        console.error("Failed to load engine settings from DB", e);
        return null;
      }
    }
    return null;
  },
  setItem: async (name: string, value: string): Promise<void> => {
    if (name === 'firelink-engine-settings') {
      try {
        await tauriStore.set('engine_settings', value);
        await tauriStore.save();
      } catch (e) {
        console.error("Failed to save engine settings to DB", e);
      }
    }
  },
  removeItem: async (_name: string): Promise<void> => {
    // no-op for now
  },
};

export interface SettingsState {
  defaultDownloadPath: string;
  globalSpeedLimit: number;
  concurrentDownloads: number;

  setDefaultDownloadPath: (path: string) => void;
  setGlobalSpeedLimit: (limit: number) => void;
  setConcurrentDownloads: (count: number) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      defaultDownloadPath: '~/Downloads',
      globalSpeedLimit: 0,
      concurrentDownloads: 3,

      setDefaultDownloadPath: (path) => {
        info(`Settings updated: defaultDownloadPath = ${path}`);
        set({ defaultDownloadPath: path });
      },
      setGlobalSpeedLimit: (limit) => {
        info(`Settings updated: globalSpeedLimit = ${limit}`);
        set({ globalSpeedLimit: limit });
      },
      setConcurrentDownloads: (count) => {
        info(`Settings updated: concurrentDownloads = ${count}`);
        set({ concurrentDownloads: count });
      },
    }),
    {
      name: 'firelink-engine-settings',
      storage: createJSONStorage(() => tauriStorage),
      partialize: (state) => ({
        defaultDownloadPath: state.defaultDownloadPath,
        globalSpeedLimit: state.globalSpeedLimit,
        concurrentDownloads: state.concurrentDownloads,
      }),
    }
  )
);
