import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useDownloadStore } from './useDownloadStore';
import * as ipc from '../ipc';

vi.mock('../ipc', () => ({
  invokeCommand: vi.fn(),
}));

// Mock window.__TAURI_INTERNALS__ and log to prevent errors
vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-store', () => {
  return {
    LazyStore: class {
      get = vi.fn().mockResolvedValue([]);
      set = vi.fn();
      save = vi.fn();
    }
  };
});

vi.mock('./useSettingsStore', () => ({
  useSettingsStore: {
    getState: vi.fn(() => ({
      proxyMode: 'none',
      siteLogins: [],
      globalSpeedLimit: '',
      perServerConnections: 16,
      customUserAgent: '',
      maxAutomaticRetries: 3,
      mediaCookieSource: 'none',
    })),
  }
}));

describe('useDownloadStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useDownloadStore.setState({
      downloads: [],
      backendRegisteredIds: new Set(),
      pendingOrder: [],
    });
  });

  it('Start Queue dispatches exactly once for mixed dispatched/undispatched items', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', status: 'queued', category: 'General', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
        { id: '2', url: 'http://test2', fileName: 'f2', status: 'queued', category: 'General', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
      backendRegisteredIds: new Set(['1']), // 1 is already registered, so it skips dispatch
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['1', '2'];
      return undefined;
    });

    const dispatched = await useDownloadStore.getState().startQueue('MAIN');
    expect(dispatched).toBe(2); // Both items counted as dispatched/handled

    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    const enqueues = calls.filter(c => c[0] === 'enqueue_download');
    expect(enqueues.length).toBe(1);
    expect((enqueues[0] as any)[1].item.id).toBe('2');
  });

  it('resumeDownload unregisters ID and re-dispatches if un-resumable', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', status: 'paused', category: 'General', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
      ] as any[],
      backendRegisteredIds: new Set(['1']),
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'resume_download') return false; // Not resumable
      if (cmd === 'get_pending_order') return ['1'];
      return undefined;
    });

    await useDownloadStore.getState().resumeDownload('1');

    // It should have called resume_download, then unregistered, then enqueue_download
    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    expect(calls.some(c => c[0] === 'resume_download')).toBe(true);
    expect(calls.some(c => c[0] === 'enqueue_download')).toBe(true);
    expect(useDownloadStore.getState().backendRegisteredIds.has('1')).toBe(true); // Re-registered by dispatchItem
  });
});
