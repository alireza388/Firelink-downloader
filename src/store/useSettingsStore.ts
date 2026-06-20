import { create } from 'zustand';
import { persist, createJSONStorage, StateStorage } from 'zustand/middleware';
import { invokeCommand as invoke } from '../ipc';
import { info } from '@tauri-apps/plugin-log';
import type { ActiveView } from '../bindings/ActiveView';
import type { AppFontSize } from '../bindings/AppFontSize';
import type { ListRowDensity } from '../bindings/ListRowDensity';
import type { MediaCookieSource } from '../bindings/MediaCookieSource';
import type { PostQueueAction } from '../bindings/PostQueueAction';
import type { PersistedSettings } from '../bindings/PersistedSettings';
import type { ProxyMode } from '../bindings/ProxyMode';
import type { SchedulerSettings } from '../bindings/SchedulerSettings';
import type { SettingsTab } from '../bindings/SettingsTab';
import type { SiteLogin } from '../bindings/SiteLogin';
import type { Theme } from '../bindings/Theme';

let settingsSave = Promise.resolve();

const tauriStorage: StateStorage = {
  getItem: async (name: string): Promise<string | null> => {
    if (name === 'firelink-settings') {
      try {
        return await invoke('db_load_settings');
      } catch (e) {
        console.error("Failed to load settings from DB", e);
        return null;
      }
    }
    return null;
  },
  setItem: async (name: string, value: string): Promise<void> => {
    if (name === 'firelink-settings') {
      settingsSave = settingsSave
        .then(() => invoke('db_save_settings', { data: value }))
        .catch(e => {
          console.error("Failed to save settings to DB", e);
        });
      await settingsSave;
    }
  },
  removeItem: async (_name: string): Promise<void> => {
    // no-op for now
  },
};

/**
 * Keychain identifier for the browser-extension pairing token. The token is an
 * HMAC shared secret and is therefore persisted via the OS keychain rather
 * than the user-data database. Legacy plaintext values are migrated into the
 * Keychain before being removed from persisted settings.
 */
const PAIRING_TOKEN_KEYCHAIN_ID = 'extension-pairing-token';

export type {
  ActiveView,
  AppFontSize,
  ListRowDensity,
  MediaCookieSource,
  PostQueueAction,
  ProxyMode,
  SchedulerSettings,
  SettingsTab,
  SiteLogin,
  Theme
};

export interface SettingsState {
  theme: Theme;
  defaultDownloadPath: string;
  maxConcurrentDownloads: number;
  globalSpeedLimit: string;
  isSidebarVisible: boolean;
  activeView: ActiveView;
  activeSettingsTab: SettingsTab;
  scheduler: SchedulerSettings;
  schedulerRunning: boolean;
  schedulerLastStartKey: string;
  schedulerLastStopKey: string;
  lastCustomSpeedLimitKiB: number;

  // Replicated SwiftUI App Settings
  perServerConnections: number;
  maxAutomaticRetries: number;
  showNotifications: boolean;
  playCompletionSound: boolean;
  appFontSize: AppFontSize;
  listRowDensity: ListRowDensity;
  showDockBadge: boolean;
  showMenuBarIcon: boolean;
  proxyMode: ProxyMode;
  proxyHost: string;
  proxyPort: number;
  customUserAgent: string;
  askWhereToSaveEachFile: boolean;
  preventsSleepWhileDownloading: boolean;
  mediaCookieSource: MediaCookieSource;
  downloadDirectories: Record<string, string>;
  siteLogins: SiteLogin[];
  extensionPairingToken: string;
  autoCheckUpdates: boolean;

  setTheme: (theme: Theme) => void;
  setDefaultDownloadPath: (path: string) => void;
  setMaxConcurrentDownloads: (count: number) => void;
  setGlobalSpeedLimit: (limit: string) => void;
  setActiveView: (view: ActiveView) => void;
  setActiveSettingsTab: (tab: SettingsTab) => void;
  setScheduler: (settings: SchedulerSettings) => void;
  setSchedulerRunning: (running: boolean) => void;
  setSchedulerLastStartKey: (key: string) => void;
  setSchedulerLastStopKey: (key: string) => void;
  setLastCustomSpeedLimitKiB: (limit: number) => void;
  toggleSidebar: () => void;

  setPerServerConnections: (count: number) => void;
  setMaxAutomaticRetries: (count: number) => void;
  setShowNotifications: (show: boolean) => void;
  setPlayCompletionSound: (play: boolean) => void;
  setAppFontSize: (size: AppFontSize) => void;
  setListRowDensity: (density: ListRowDensity) => void;
  setShowDockBadge: (show: boolean) => void;
  setShowMenuBarIcon: (show: boolean) => void;
  setProxyMode: (mode: ProxyMode) => void;
  setProxyHost: (host: string) => void;
  setProxyPort: (port: number) => void;
  setCustomUserAgent: (userAgent: string) => void;
  setAskWhereToSaveEachFile: (ask: boolean) => void;
  setPreventsSleepWhileDownloading: (prevent: boolean) => void;
  setMediaCookieSource: (source: MediaCookieSource) => void;
  setCategoryDirectory: (category: string, path: string) => void;
  resetCategoryDirectories: () => void;
  addSiteLogin: (login: SiteLogin) => void;
  removeSiteLogin: (id: string) => void;
  regeneratePairingToken: () => void;
  setAutoCheckUpdates: (autoCheckUpdates: boolean) => void;
  hydratePairingToken: () => Promise<boolean>;
}

const defaultDirectories = {
  Musics: '~/Downloads/Musics',
  Movies: '~/Downloads/Movies',
  Compressed: '~/Downloads/Compressed',
  Documents: '~/Downloads/Documents',
  Pictures: '~/Downloads/Pictures',
  Applications: '~/Downloads/Applications',
  Other: '~/Downloads/Other'
};

const normalizeDownloadDirectories = (directories: unknown): Record<string, string> => {
  if (!directories || typeof directories !== 'object') {
    return { ...defaultDirectories };
  }

  const values = directories as Record<string, unknown>;
  const directory = (current: string, legacy?: string) => {
    const value = values[current] ?? (legacy ? values[legacy] : undefined);
    return typeof value === 'string' && value.length > 0
      ? value
      : defaultDirectories[current as keyof typeof defaultDirectories];
  };

  return {
    Musics: directory('Musics', 'Audio'),
    Movies: directory('Movies', 'Video'),
    Compressed: directory('Compressed', 'Archives'),
    Documents: directory('Documents'),
    Pictures: directory('Pictures', 'Images'),
    Applications: directory('Applications', 'Apps'),
    Other: directory('Other')
  };
};

const generateSecureToken = () => {
  try {
    const cryptoObj = typeof window !== 'undefined'
      ? (window as Window & { msCrypto?: Crypto }).crypto
        || (window as Window & { msCrypto?: Crypto }).msCrypto
      : null;
    if (cryptoObj && cryptoObj.getRandomValues) {
      const arr = new Uint8Array(24);
      cryptoObj.getRandomValues(arr);
      let binary = '';
      for (let i = 0; i < arr.byteLength; i++) {
        binary += String.fromCharCode(arr[i]);
      }
      return btoa(binary);
    }
  } catch (e) {
    console.warn("Secure token generation failed, falling back to random characters", e);
  }
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  let token = '';
  for (let i = 0; i < 32; i++) {
    token += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return token;
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      theme: 'system',
      defaultDownloadPath: '~/Downloads',
      maxConcurrentDownloads: 3,
      globalSpeedLimit: '',
      activeView: 'downloads',
      activeSettingsTab: 'downloads',
      isSidebarVisible: true,
      scheduler: {
        enabled: false,
        startTime: '00:00',
        stopTimeEnabled: false,
        stopTime: '08:00',
        everyday: true,
        selectedDays: [0, 1, 2, 3, 4, 5, 6],
        postQueueAction: 'none'
      },
      schedulerRunning: false,
      schedulerLastStartKey: '',
      schedulerLastStopKey: '',
      lastCustomSpeedLimitKiB: 1024,

      // Replicated SwiftUI defaults
      perServerConnections: 16,
      maxAutomaticRetries: 3,
      showNotifications: true,
      playCompletionSound: true,
      appFontSize: 'standard',
      listRowDensity: 'standard',
      showDockBadge: true,
      showMenuBarIcon: true,
      proxyMode: 'none',
      proxyHost: '',
      proxyPort: 8080,
      customUserAgent: '',
      askWhereToSaveEachFile: false,
      preventsSleepWhileDownloading: true,
      mediaCookieSource: 'none',
      downloadDirectories: { ...defaultDirectories },
      siteLogins: [],
      extensionPairingToken: '',
      autoCheckUpdates: true,

      setTheme: (theme) => { info('Settings updated: theme'); set({ theme }); },
      setDefaultDownloadPath: (path) => { info('Settings updated: defaultDownloadPath'); set({ defaultDownloadPath: path }); },
      setMaxConcurrentDownloads: (max) => {
        info('Settings updated: maxConcurrentDownloads');
        set({ maxConcurrentDownloads: max });
      },
      setGlobalSpeedLimit: (limit) => {
        info('Settings updated: globalSpeedLimit');
        set({ globalSpeedLimit: limit });
      },
      setActiveView: (view) => set({ activeView: view }),
      setActiveSettingsTab: (activeSettingsTab) => set({ activeSettingsTab }),
      setScheduler: (scheduler) => set({ scheduler }),
      setSchedulerRunning: (schedulerRunning) => set({ schedulerRunning }),
      setSchedulerLastStartKey: (schedulerLastStartKey) => set({ schedulerLastStartKey }),
      setSchedulerLastStopKey: (schedulerLastStopKey) => set({ schedulerLastStopKey }),
      setLastCustomSpeedLimitKiB: (lastCustomSpeedLimitKiB) => set({ lastCustomSpeedLimitKiB }),
      toggleSidebar: () => set((state) => ({ isSidebarVisible: !state.isSidebarVisible })),

      setPerServerConnections: (perServerConnections) => set({ perServerConnections }),
      setMaxAutomaticRetries: (maxAutomaticRetries) => set({ maxAutomaticRetries }),
      setShowNotifications: (showNotifications) => set({ showNotifications }),
      setPlayCompletionSound: (playCompletionSound) => set({ playCompletionSound }),
      setAppFontSize: (appFontSize) => set({ appFontSize }),
      setListRowDensity: (listRowDensity) => set({ listRowDensity }),
      setShowDockBadge: (showDockBadge) => {
        set({ showDockBadge });
        if (!showDockBadge) invoke('update_dock_badge', { count: 0 }).catch(console.error);
      },
      setShowMenuBarIcon: (showMenuBarIcon) => set({ showMenuBarIcon }),
      setProxyMode: (proxyMode) => set({ proxyMode }),
      setProxyHost: (proxyHost) => set({ proxyHost }),
      setProxyPort: (proxyPort) => set({ proxyPort }),
      setCustomUserAgent: (customUserAgent) => set({ customUserAgent }),
      setAskWhereToSaveEachFile: (askWhereToSaveEachFile) => set({ askWhereToSaveEachFile }),
      setPreventsSleepWhileDownloading: (preventsSleepWhileDownloading) => {
        info('Settings updated: preventsSleepWhileDownloading');
        set({ preventsSleepWhileDownloading });
        if (!preventsSleepWhileDownloading) invoke('set_prevent_sleep', { prevent: false }).catch(console.error);
      },
      setMediaCookieSource: (mediaCookieSource) => { info('Settings updated: mediaCookieSource'); set({ mediaCookieSource }); },
      setCategoryDirectory: (category, path) => {
        info(`Settings updated: category directory ${category}`);
        set((state) => ({
          downloadDirectories: { ...state.downloadDirectories, [category]: path }
        }));
      },
      resetCategoryDirectories: () => { info('Settings updated: resetCategoryDirectories'); set({ downloadDirectories: { ...defaultDirectories } }); },
      addSiteLogin: (login) => set((state) => ({
        siteLogins: [...state.siteLogins, login]
      })),
      removeSiteLogin: (id) => set((state) => ({
        siteLogins: state.siteLogins.filter((login) => login.id !== id)
      })),
      regeneratePairingToken: () => {
        const token = generateSecureToken();
        set({ extensionPairingToken: token });
        invoke('set_keychain_password', { id: PAIRING_TOKEN_KEYCHAIN_ID, password: token }).catch(e => {
          console.error('Failed to persist regenerated extension pairing token to keychain:', e);
        });
      },
      hydratePairingToken: async () => {
        const result = await invoke('hydrate_extension_pairing_token');
        set({ extensionPairingToken: result.token });
        return result.tokenChanged;
      },
      setAutoCheckUpdates: (autoCheckUpdates) => set({ autoCheckUpdates }),
    }),
    {
      name: 'firelink-settings',
      storage: createJSONStorage(() => tauriStorage),
      version: 1,
      migrate: (persistedState) => {
        if (!persistedState || typeof persistedState !== 'object') {
          return persistedState as SettingsState;
        }
        const persisted = persistedState as Partial<SettingsState>;
        return {
          ...persisted,
          downloadDirectories: normalizeDownloadDirectories(persisted.downloadDirectories),
          siteLogins: Array.isArray(persisted.siteLogins) ? persisted.siteLogins : []
        } as SettingsState;
      },
      partialize: (state): PersistedSettings => ({
        theme: state.theme,
        defaultDownloadPath: state.defaultDownloadPath,
        maxConcurrentDownloads: state.maxConcurrentDownloads,
        globalSpeedLimit: state.globalSpeedLimit,
        isSidebarVisible: state.isSidebarVisible,
        activeSettingsTab: state.activeSettingsTab,
        scheduler: state.scheduler,
        schedulerLastStartKey: state.schedulerLastStartKey,
        schedulerLastStopKey: state.schedulerLastStopKey,
        lastCustomSpeedLimitKiB: state.lastCustomSpeedLimitKiB,
        
        perServerConnections: state.perServerConnections,
        maxAutomaticRetries: state.maxAutomaticRetries,
        showNotifications: state.showNotifications,
        playCompletionSound: state.playCompletionSound,
        appFontSize: state.appFontSize,
        listRowDensity: state.listRowDensity,
        showDockBadge: state.showDockBadge,
        showMenuBarIcon: state.showMenuBarIcon,
        proxyMode: state.proxyMode,
        proxyHost: state.proxyHost,
        proxyPort: state.proxyPort,
        customUserAgent: state.customUserAgent,
        askWhereToSaveEachFile: state.askWhereToSaveEachFile,
        preventsSleepWhileDownloading: state.preventsSleepWhileDownloading,
        mediaCookieSource: state.mediaCookieSource,
        downloadDirectories: state.downloadDirectories,
        siteLogins: state.siteLogins,
        autoCheckUpdates: state.autoCheckUpdates
      }),
      merge: (persistedState: unknown, currentState) => {
        const persisted = persistedState && typeof persistedState === 'object'
          ? persistedState as Partial<SettingsState>
          : {};
        return ({
        ...currentState,
        ...persisted,
        appFontSize: persisted.appFontSize || currentState.appFontSize,
        listRowDensity: persisted.listRowDensity || currentState.listRowDensity,
        downloadDirectories: normalizeDownloadDirectories(persisted.downloadDirectories),
        siteLogins: Array.isArray(persisted.siteLogins)
          ? persisted.siteLogins
          : currentState.siteLogins
        });
      }
    }
  )
);
