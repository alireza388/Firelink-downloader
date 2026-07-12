import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getProxyArgs, getSiteLogin, normalizeCustomProxy, useDownloadStore } from './useDownloadStore';
import { useSettingsStore } from './useSettingsStore';
import * as ipc from '../ipc';

vi.mock('../ipc', () => ({
  invokeCommand: vi.fn(),
}));

// Mock window.__TAURI_INTERNALS__ and log to prevent errors
vi.mock('../utils/logger', () => ({
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
}));

vi.mock('@tauri-apps/api/path', () => ({
  homeDir: vi.fn().mockResolvedValue('/Users/test'),
  join: vi.fn(async (...parts: string[]) =>
    parts
      .map((part, index) => index === 0 ? part.replace(/[\\/]+$/, '') : part.replace(/^[\\/]+|[\\/]+$/g, ''))
      .join('/')
  ),
}));

vi.mock('./useSettingsStore', () => ({
  useSettingsStore: {
    getState: vi.fn(() => ({
      proxyMode: 'none',
      siteLogins: [],
      globalSpeedLimit: '',
      speedLimitPresetValues: [1, 5, 10],
      logsEnabled: false,
      perServerConnections: 16,
      customUserAgent: '',
      maxAutomaticRetries: 3,
      mediaCookieSource: 'none',
      baseDownloadFolder: '~/Downloads',
      categorySubfoldersEnabled: true,
      categorySubfolders: {
        Musics: 'Musics',
        Movies: 'Movies',
        Compressed: 'Compressed',
        Documents: 'Documents',
        Pictures: 'Pictures',
        Applications: 'Applications',
        Other: 'Other',
      },
      categoryDirectoryOverrides: {},
    })),
  }
}));

describe('useDownloadStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useSettingsStore.getState).mockReturnValue({
      proxyMode: 'none',
      siteLogins: [],
      globalSpeedLimit: '',
      speedLimitPresetValues: [1, 5, 10],
      logsEnabled: false,
      perServerConnections: 16,
      customUserAgent: '',
      maxAutomaticRetries: 3,
      mediaCookieSource: 'none',
      baseDownloadFolder: '~/Downloads',
      categorySubfoldersEnabled: true,
      categorySubfolders: {
        Musics: 'Musics',
        Movies: 'Movies',
        Compressed: 'Compressed',
        Documents: 'Documents',
        Pictures: 'Pictures',
        Applications: 'Applications',
        Other: 'Other',
      },
      categoryDirectoryOverrides: {},
    } as unknown as ReturnType<typeof useSettingsStore.getState>);
    useDownloadStore.setState({
      downloads: [],
      backendRegisteredIds: new Set(),
      pendingOrder: [],
      isAddModalOpen: false,
      pendingAddUrls: '',
      pendingAddReferer: '',
      pendingAddFilename: '',
      pendingAddHeaders: '',
      pendingAddCookies: '',
      pendingAddMediaUrls: [],
      pendingAddRequestContexts: {},
      pendingAddRequestVersion: 0,
    });
  });

  it('invalidates in-flight Add-modal handoffs when the modal is toggled', () => {
    const initialVersion = useDownloadStore.getState().pendingAddRequestVersion;

    useDownloadStore.getState().toggleAddModal(true);
    expect(useDownloadStore.getState().pendingAddRequestVersion).toBe(initialVersion + 1);

    useDownloadStore.getState().toggleAddModal(false);
    expect(useDownloadStore.getState().pendingAddRequestVersion).toBe(initialVersion + 2);
  });

  it('normalizes proxy settings for download dispatch', async () => {
    expect(normalizeCustomProxy('127.0.0.1', 8080)).toBe('http://127.0.0.1:8080');
    expect(normalizeCustomProxy('http://proxy.local:9000', 8080)).toBe('http://proxy.local:9000');
    expect(normalizeCustomProxy(' socks5://127.0.0.1 ', 1080)).toBeNull();
    expect(normalizeCustomProxy('https://proxy.local', 8443)).toBeNull();
    expect(normalizeCustomProxy('127.0.0.1', NaN)).toBeNull();

    expect(await getProxyArgs({
      proxyMode: 'none',
      proxyHost: '',
      proxyPort: 8080
    } as ReturnType<typeof useSettingsStore.getState>)).toBe('none');

    vi.mocked(ipc.invokeCommand).mockResolvedValueOnce(null);
    expect(await getProxyArgs({
      proxyMode: 'system',
      proxyHost: '',
      proxyPort: 8080
    } as ReturnType<typeof useSettingsStore.getState>)).toBe('none');

    expect(await getProxyArgs({
      proxyMode: 'custom',
      proxyHost: 'http://127.0.0.1',
      proxyPort: 1080
    } as ReturnType<typeof useSettingsStore.getState>)).toBe('http://127.0.0.1:1080');
  });

  it('matches site logins by host, wildcard host, path, and full URL patterns', () => {
    const settings = {
      siteLogins: [
        { id: 'host', urlPattern: 'example.com', username: 'host' },
        { id: 'wildcard', urlPattern: '*.cdn.example.com', username: 'wildcard' },
        { id: 'broad', urlPattern: '*.example.com', username: 'broad' },
        { id: 'path', urlPattern: 'secure.example.com/private/*', username: 'path' },
        { id: 'url', urlPattern: 'https://downloads.example.net/releases/*', username: 'url' }
      ]
    } as ReturnType<typeof useSettingsStore.getState>;

    expect(getSiteLogin('https://example.com/file.zip', settings)?.id).toBe('host');
    expect(getSiteLogin('https://assets.cdn.example.com/file.zip', settings)?.id).toBe('wildcard');
    expect(getSiteLogin('https://secure.example.com/private/file.zip', settings)?.id).toBe('path');
    expect(getSiteLogin('https://downloads.example.net/releases/app.zip', settings)?.id).toBe('url');
    expect(getSiteLogin('https://secure.example.com/public/file.zip', settings)?.id).toBe('broad');
    expect(getSiteLogin('https://unrelated.example.org/public/file.zip', settings)).toBeNull();
  });

  it('Start Queue dispatches exactly once for mixed dispatched/undispatched items', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
        { id: '2', url: 'http://test2', fileName: 'f2', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
      backendRegisteredIds: new Set(['1']), // 1 is already registered, so it skips dispatch
      pendingOrder: ['1'],
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['1', '2'];
      return undefined;
    });

    const dispatched = await useDownloadStore.getState().startQueue('MAIN');
    expect(dispatched).toEqual(['1', '2']);

    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    const enqueues = calls.filter(c => c[0] === 'enqueue_download');
    expect(enqueues.length).toBe(1);
    expect((enqueues[0] as any)[1].item.id).toBe('2');
  });

  it('repairs stale queued backend registrations before accepting a queue start', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'stale', url: 'http://test', fileName: 'f', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
      ] as any[],
      backendRegisteredIds: new Set(['stale']),
      pendingOrder: [],
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'resume_download') return false;
      if (cmd === 'get_pending_order') return ['stale'];
      return undefined;
    });

    expect(await useDownloadStore.getState().startQueue('MAIN')).toEqual(['stale']);

    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    expect(calls.some(call => call[0] === 'resume_download')).toBe(true);
    expect(calls.some(call => call[0] === 'enqueue_download')).toBe(true);
  });

  it('does not overwrite a downloading event received while starting a queue', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'enqueue_download') {
        useDownloadStore.getState().updateDownload('1', {
          status: 'downloading',
          speed: '1 MB/s',
          eta: '10s'
        });
      }
      if (cmd === 'get_pending_order') return ['1'];
      return undefined;
    });

    expect(await useDownloadStore.getState().startQueue('MAIN')).toEqual(['1']);
    expect(useDownloadStore.getState().downloads[0]).toMatchObject({
      status: 'downloading',
      speed: '1 MB/s',
      eta: '10s',
      hasBeenDispatched: true
    });
  });

  it('does not resurrect a row removed while its backend enqueue is in flight', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'late', url: 'http://test', fileName: 'late.bin', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
    });

    let resolveEnqueue!: (value: { id: string; filename: string }) => void;
    const enqueue = new Promise<{ id: string; filename: string }>(resolve => {
      resolveEnqueue = resolve;
    });
    vi.mocked(ipc.invokeCommand).mockImplementation((command: string) => {
      if (command === 'enqueue_download') return enqueue as never;
      if (command === 'get_pending_order') return Promise.resolve(['late']) as never;
      return Promise.resolve(undefined) as never;
    });

    const start = useDownloadStore.getState().startQueue('MAIN');
    await vi.waitFor(() => {
      expect(ipc.invokeCommand).toHaveBeenCalledWith(
        'enqueue_download',
        expect.objectContaining({ item: expect.objectContaining({ id: 'late' }) })
      );
    });

    await useDownloadStore.getState().removeDownload('late');
    resolveEnqueue({ id: 'late', filename: 'late.bin' });

    await expect(start).resolves.toEqual([]);
    expect(useDownloadStore.getState().downloads).toEqual([]);
    expect(useDownloadStore.getState().backendRegisteredIds.has('late')).toBe(false);
    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.filter(([command]) => command === 'remove_download')
    ).toHaveLength(2);
  });

  it('re-enqueues the edited values only after an obsolete queued dispatch is removed', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'edited', url: 'http://test', fileName: 'old.bin', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
    });

    let resolveFirstEnqueue!: (value: { id: string; filename: string }) => void;
    const firstEnqueue = new Promise<{ id: string; filename: string }>(resolve => {
      resolveFirstEnqueue = resolve;
    });
    let enqueueCount = 0;
    vi.mocked(ipc.invokeCommand).mockImplementation((command: string) => {
      if (command === 'enqueue_download') {
        enqueueCount += 1;
        return (enqueueCount === 1
          ? firstEnqueue
          : Promise.resolve({ id: 'edited', filename: 'new.bin' })) as never;
      }
      if (command === 'get_pending_order') return Promise.resolve(['edited']) as never;
      return Promise.resolve(undefined) as never;
    });

    const start = useDownloadStore.getState().startQueue('MAIN');
    await vi.waitFor(() => expect(enqueueCount).toBe(1));
    const update = useDownloadStore.getState().applyProperties('edited', { fileName: 'new.bin' });
    resolveFirstEnqueue({ id: 'edited', filename: 'old.bin' });

    await expect(update).resolves.toBeUndefined();
    await expect(start).resolves.toEqual([]);
    expect(enqueueCount).toBe(2);
    expect(vi.mocked(ipc.invokeCommand)).toHaveBeenCalledWith(
      'get_pending_order',
      { queueId: 'MAIN' }
    );
    expect(useDownloadStore.getState().downloads[0]).toMatchObject({
      fileName: 'new.bin',
      hasBeenDispatched: true,
    });
  });

  it('settles an in-flight dispatch before pausing so it cannot start after pause succeeds', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'paused', url: 'http://test', fileName: 'paused.bin', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
    });

    let resolveEnqueue!: (value: { id: string; filename: string }) => void;
    const enqueue = new Promise<{ id: string; filename: string }>(resolve => {
      resolveEnqueue = resolve;
    });
    vi.mocked(ipc.invokeCommand).mockImplementation((command: string) => {
      if (command === 'enqueue_download') return enqueue as never;
      return Promise.resolve(undefined) as never;
    });

    const start = useDownloadStore.getState().startQueue('MAIN');
    await vi.waitFor(() => {
      expect(ipc.invokeCommand).toHaveBeenCalledWith(
        'enqueue_download',
        expect.objectContaining({ item: expect.objectContaining({ id: 'paused' }) })
      );
    });
    const pause = useDownloadStore.getState().pauseDownload('paused');
    await vi.waitFor(() => {
      expect(ipc.invokeCommand).toHaveBeenCalledWith(
        'cancel_enqueue_generation',
        expect.objectContaining({ id: 'paused' })
      );
    });
    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.some(([command]) => command === 'pause_download')
    ).toBe(false);
    resolveEnqueue({ id: 'paused', filename: 'paused.bin' });

    await expect(pause).resolves.toBeUndefined();
    await expect(start).resolves.toEqual([]);
    expect(useDownloadStore.getState().downloads[0]).toMatchObject({ status: 'paused' });
    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.filter(([command]) => command === 'remove_download')
    ).toHaveLength(1);
    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.filter(([command]) => command === 'pause_download')
    ).toHaveLength(1);
  });

  it('resumeDownload unregisters ID and re-dispatches if un-resumable', async () => {
    let enqueueGeneration: string | undefined;
    useDownloadStore.setState({
      downloads: [
        { id: 'resume-generation', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'paused', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
      ] as any[],
      backendRegisteredIds: new Set(['resume-generation']),
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'resume_download') return false; // Not resumable
      if (cmd === 'enqueue_download') {
        enqueueGeneration = (args as { item: { lifecycle_generation: string } }).item.lifecycle_generation;
        return { id: 'resume-generation', filename: 'f1' };
      }
      if (cmd === 'get_pending_order') return ['resume-generation'];
      return undefined;
    });

    await useDownloadStore.getState().resumeDownload('resume-generation');

    // It should have called resume_download, then unregistered, then enqueue_download
    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    expect(calls.some(c => c[0] === 'resume_download')).toBe(true);
    expect(calls.some(c => c[0] === 'enqueue_download')).toBe(true);
    expect(enqueueGeneration).toBe('1');
    expect(useDownloadStore.getState().downloads[0].lastTry).toEqual(expect.any(String));
    expect(useDownloadStore.getState().backendRegisteredIds.has('resume-generation')).toBe(true); // Re-registered by dispatchItem
  });

  it('does not re-enqueue when the existing resume RPC fails', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'resume-rpc-error', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'paused', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
      ] as any[],
      backendRegisteredIds: new Set(['resume-rpc-error']),
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'resume_download') throw new Error('aria2 RPC unavailable');
      return undefined;
    });

    await expect(useDownloadStore.getState().resumeDownload('resume-rpc-error')).resolves.toBe(false);

    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.some(([command]) => command === 'enqueue_download')
    ).toBe(false);
    expect(useDownloadStore.getState().downloads[0]).toMatchObject({
      status: 'paused',
      lastTry: expect.any(String),
    });
  });

  it('cleans an accepted backend enqueue when queue reconciliation fails', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'enqueue-reconcile-error', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'enqueue_download') return { id: 'enqueue-reconcile-error', filename: 'f1' };
      if (cmd === 'get_pending_order') throw new Error('queue state unavailable');
      return undefined;
    });

    await expect(useDownloadStore.getState().startQueue('MAIN')).resolves.toEqual([]);

    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.some(([command]) => command === 'remove_download')
    ).toBe(true);
    expect(useDownloadStore.getState().downloads[0]).toMatchObject({
      status: 'failed',
      lastError: 'queue state unavailable',
    });
  });


  it('adds to the selected queue without dispatching', async () => {
    await useDownloadStore.getState().addDownload({
      id: 'queue-1',
      url: 'https://example.com/queue.bin',
      fileName: 'queue.bin',
      category: 'Other',
      dateAdded: ''
    }, { type: 'add-to-queue', queueId: 'queue-b' });

    const item = useDownloadStore.getState().downloads[0];
    expect(item.status).toBe('staged');
    expect(item.queueId).toBe('queue-b');
    expect(item.queuePosition).toBe(0);
    expect(ipc.invokeCommand).not.toHaveBeenCalledWith('enqueue_download', expect.anything());
  });

  it('starts immediately in the main queue', async () => {
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['start-1'];
      return undefined;
    });

    await useDownloadStore.getState().addDownload({
      id: 'start-1',
      url: 'https://example.com/start.bin',
      fileName: 'start.bin',
      category: 'Other',
      dateAdded: ''
    }, { type: 'start-now' });

    const item = useDownloadStore.getState().downloads[0];
    expect(item.queueId).toBe('00000000-0000-0000-0000-000000000001');
    expect(item.hasBeenDispatched).toBe(true);
    expect(ipc.invokeCommand).toHaveBeenCalledWith(
      'enqueue_download',
      expect.objectContaining({
        item: expect.objectContaining({ id: 'start-1' })
      })
    );
  });

  it('does not replace an explicit no-limit item speed with the global speed limit', async () => {
    const defaultSettings = useSettingsStore.getState();
    vi.mocked(useSettingsStore.getState).mockReturnValue({
      ...defaultSettings,
      globalSpeedLimit: '2M'
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['uncapped'];
      return undefined;
    });

    await useDownloadStore.getState().addDownload({
      id: 'uncapped',
      url: 'https://example.com/uncapped.bin',
      fileName: 'uncapped.bin',
      category: 'Other',
      dateAdded: '',
      speedLimit: '0'
    }, { type: 'start-now' });

    expect(ipc.invokeCommand).toHaveBeenCalledWith(
      'enqueue_download',
      expect.objectContaining({
        item: expect.objectContaining({
          id: 'uncapped',
          speed_limit: '0'
        })
      })
    );
  });

  it('uses the global speed limit only when an item has no explicit speed override', async () => {
    const defaultSettings = useSettingsStore.getState();
    vi.mocked(useSettingsStore.getState).mockReturnValue({
      ...defaultSettings,
      globalSpeedLimit: '2M'
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['inherits-global'];
      return undefined;
    });

    await useDownloadStore.getState().addDownload({
      id: 'inherits-global',
      url: 'https://example.com/inherits-global.bin',
      fileName: 'inherits-global.bin',
      category: 'Other',
      dateAdded: ''
    }, { type: 'start-now' });

    expect(ipc.invokeCommand).toHaveBeenCalledWith(
      'enqueue_download',
      expect.objectContaining({
        item: expect.objectContaining({
          id: 'inherits-global',
          speed_limit: '2M'
        })
      })
    );
  });

  it('reports a rejected immediate start instead of claiming success', async () => {
    vi.mocked(ipc.invokeCommand).mockImplementation(async (command: string) => {
      if (command === 'enqueue_download') {
        throw new Error('backend unavailable');
      }
      return undefined;
    });

    const added = await useDownloadStore.getState().addDownload({
      id: 'rejected-start',
      url: 'https://example.com/rejected.bin',
      fileName: 'rejected.bin',
      category: 'Other',
      dateAdded: ''
    }, { type: 'start-now' });

    expect(added).toBe(false);
    expect(useDownloadStore.getState().downloads[0].status).toBe('failed');
    expect(useDownloadStore.getState().downloads[0].lastError).toBe('backend unavailable');
  });

  it('preserves backend rejection reasons while auto-resuming saved queued items', async () => {
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'db_get_all_queues') return [];
      if (cmd === 'db_get_all_downloads') {
        return [JSON.stringify({
          id: 'startup-failed',
          url: 'https://example.com/file.bin',
          fileName: 'file.bin',
          status: 'queued',
          category: 'Other',
          dateAdded: '',
          queueId: '00000000-0000-0000-0000-000000000001',
          hasBeenDispatched: true
        })];
      }
      if (cmd === 'enqueue_many') {
        return [{
          id: 'startup-failed',
          success: false,
          error: 'aria2 addUri failed: connection refused'
        }];
      }
      if (cmd === 'get_pending_order') return [];
      return undefined;
    });

    await useDownloadStore.getState().initDB();

    expect(useDownloadStore.getState().downloads[0]).toMatchObject({
      status: 'failed',
      lastError: 'aria2 addUri failed: connection refused'
    });
  });

  it('redownloads fallback media without requiring a format selector', async () => {
    useDownloadStore.setState({
      downloads: [{
        id: 'media-fallback',
        url: 'https://youtube.com/watch?v=test',
        fileName: 'watch',
        destination: '/tmp',
        status: 'completed',
        category: 'Other',
        dateAdded: '',
        isMedia: true
      }] as any[]
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return [];
      return undefined;
    });

    await useDownloadStore.getState().redownload('media-fallback');

    expect(ipc.invokeCommand).toHaveBeenCalledWith(
      'enqueue_download',
      expect.objectContaining({
        item: expect.objectContaining({
          is_media: true,
          format_selector: null
        })
      })
    );
  });

  it('starts and pauses all items regardless of legacy missing queue ids', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'ready', url: 'http://ready', fileName: 'ready', status: 'ready', category: 'Other', dateAdded: '' },
        { id: 'active', url: 'http://active', fileName: 'active', status: 'processing', category: 'Other', dateAdded: '' },
      ] as any[],
    });

    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_pending_order') return ['ready'];
      return undefined;
    });

    expect(await useDownloadStore.getState().startAll()).toBe(1);
    expect(await useDownloadStore.getState().pauseAll()).toBe(2);

    const calls = vi.mocked(ipc.invokeCommand).mock.calls;
    expect(calls.some(call => call[0] === 'enqueue_download')).toBe(true);
    expect(calls.some(call => call[0] === 'pause_download' && (call[1] as any).id === 'active')).toBe(true);
  });

  it('migrates legacy downloads without queue ids into the main queue', async () => {
    vi.mocked(ipc.invokeCommand).mockImplementation(async (cmd: string) => {
      if (cmd === 'db_get_all_queues') return [];
      if (cmd === 'db_get_all_downloads') {
        return [JSON.stringify({
          id: 'legacy',
          url: 'https://example.com/legacy.bin',
          fileName: 'legacy.bin',
          status: 'ready',
          category: 'Other',
          dateAdded: ''
        })];
      }
      return undefined;
    });

    await useDownloadStore.getState().initDB();

    expect(useDownloadStore.getState().downloads[0].queueId)
      .toBe('00000000-0000-0000-0000-000000000001');
  });

  it('pauses queued, downloading, processing, and retrying queue items', async () => {
    useDownloadStore.setState({
      downloads: ['queued', 'downloading', 'processing', 'retrying'].map((status, index) => ({
        id: `${index}`,
        url: `https://example.com/${index}`,
        fileName: `${index}.bin`,
        status,
        category: 'Other',
        dateAdded: '',
        queueId: 'queue-a'
      })) as any[]
    });
    vi.mocked(ipc.invokeCommand).mockResolvedValue(undefined as never);

    expect(await useDownloadStore.getState().pauseQueue('queue-a')).toBe(4);
    expect(
      vi.mocked(ipc.invokeCommand).mock.calls.filter(call => call[0] === 'pause_download')
    ).toHaveLength(4);
  });

  it('assigns selected unfinished downloads to a queue without moving completed items', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'ready', status: 'ready', queueId: 'old' },
        { id: 'done', status: 'completed', queueId: 'old' }
      ] as any[]
    });

    await useDownloadStore.getState().assignToQueue(['ready', 'done'], 'new');

    expect(useDownloadStore.getState().downloads.find(item => item.id === 'ready')?.queueId).toBe('new');
    expect(useDownloadStore.getState().downloads.find(item => item.id === 'done')?.queueId).toBe('old');
  });

  it('disables scheduler when its last selected queue is deleted', async () => {
    const originalSettings = useSettingsStore.getState();
    const setScheduler = vi.fn();
    vi.mocked(useSettingsStore.getState).mockReturnValue({
      scheduler: {
        enabled: true,
        selectedQueueIds: ['queue-a']
      },
      setScheduler
    } as any);
    useDownloadStore.setState({
      queues: [
        { id: '00000000-0000-0000-0000-000000000001', name: 'Main Queue', isMain: true },
        { id: 'queue-a', name: 'Scheduled', isMain: false }
      ],
      downloads: []
    });

    await useDownloadStore.getState().removeQueue('queue-a');

    expect(setScheduler).toHaveBeenCalledWith(expect.objectContaining({
      enabled: false,
      selectedQueueIds: []
    }));
    vi.mocked(useSettingsStore.getState).mockReturnValue(originalSettings);
  });

  it('retains the UI item when backend removal fails', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'active', url: 'https://example.com/file', fileName: 'file', status: 'downloading', category: 'Other', dateAdded: '', queueId: 'main' }
      ] as any[]
    });
    vi.mocked(ipc.invokeCommand).mockImplementation((command: string) => {
      if (command === 'remove_download') {
        return Promise.reject(new Error('writer did not stop')) as never;
      }
      return Promise.resolve(undefined) as never;
    });

    await expect(useDownloadStore.getState().removeDownload('active', true))
      .rejects.toThrow('writer did not stop');
    expect(useDownloadStore.getState().downloads.map(download => download.id))
      .toEqual(['active']);
  });

  it('asks the backend to preserve resumable assets during replacement removal', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'paused', url: 'https://example.com/file', fileName: 'file', status: 'paused', category: 'Other', dateAdded: '' }
      ] as any[]
    });
    vi.mocked(ipc.invokeCommand).mockResolvedValue(undefined as never);

    await useDownloadStore.getState().removeDownload('paused', true, true);

    expect(ipc.invokeCommand).toHaveBeenCalledWith('remove_download', {
      id: 'paused',
      deleteAssets: true,
      preserveResumable: true
    });
  });

  it('starts staged queue items in their persisted queue order', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'later', url: 'https://example.com/later', fileName: 'later', status: 'staged', category: 'Other', dateAdded: '', queueId: 'queue-a', queuePosition: 1 },
        { id: 'first', url: 'https://example.com/first', fileName: 'first', status: 'staged', category: 'Other', dateAdded: '', queueId: 'queue-a', queuePosition: 0 }
      ] as any[]
    });
    vi.mocked(ipc.invokeCommand).mockImplementation(async (command: string, args?: unknown) => {
      if (command === 'get_pending_order') {
        return [(args as { queueId: string }).queueId === 'queue-a' ? 'first' : 'later'];
      }
      return undefined;
    });

    expect(await useDownloadStore.getState().startQueue('queue-a')).toEqual(['first', 'later']);
    const enqueuedIds = vi.mocked(ipc.invokeCommand).mock.calls
      .filter(call => call[0] === 'enqueue_download')
      .map(call => (call[1] as any).item.id);
    expect(enqueuedIds).toEqual(['first', 'later']);
    expect((vi.mocked(ipc.invokeCommand).mock.calls.find(call =>
      call[0] === 'enqueue_download'
    )?.[1] as any).item.queue_id).toBe('queue-a');
  });

  it('preserves extension request headers and cookies for the Add modal', async () => {
    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://example.com/file.bin'],
      referer: 'https://example.com/page',
      silent: false,
      filename: 'file.bin',
      headers: 'X-Test: value',
      cookies: 'session=secret',
      media: false
    });

    const state = useDownloadStore.getState();
    expect(state.isAddModalOpen).toBe(true);
    expect(state.pendingAddUrls).toBe('https://example.com/file.bin');
    expect(state.pendingAddReferer).toBe('https://example.com/page');
    expect(state.pendingAddFilename).toBe('file.bin');
	  expect(state.pendingAddHeaders).toBe('X-Test: value');
	  expect(state.pendingAddCookies).toBe('session=secret');
    expect(state.pendingAddMediaUrls).toEqual([]);
	 });

 it('does not reuse stale extension metadata for a later single-link handoff', async () => {
  useDownloadStore.setState({
   isAddModalOpen: true,
   pendingAddUrls: '',
   pendingAddReferer: 'https://old.example/page',
	   pendingAddFilename: '7aae36e6-00ec-4e7d-8dec-f14ace170bdb',
	   pendingAddHeaders: 'X-Old: value',
	   pendingAddCookies: 'old=session',
      pendingAddMediaUrls: []
	  });

  await useDownloadStore.getState().handleExtensionDownload({
   urls: ['https://github.com/center2055/OnionHop/releases/download/v3.5/OnionHop-3.5-macOS-arm64.dmg'],
   referer: 'https://github.com/center2055/OnionHop/releases/tag/v3.5',
   silent: false,
	   filename: null,
	   headers: 'User-Agent: Firefox Test',
	   cookies: null,
      media: false
	  });

  const state = useDownloadStore.getState();
  expect(state.pendingAddUrls).toBe('https://github.com/center2055/OnionHop/releases/download/v3.5/OnionHop-3.5-macOS-arm64.dmg');
  expect(state.pendingAddReferer).toBe('https://github.com/center2055/OnionHop/releases/tag/v3.5');
  expect(state.pendingAddFilename).toBe('');
	  expect(state.pendingAddHeaders).toBe('User-Agent: Firefox Test');
	  expect(state.pendingAddCookies).toBe('');
    expect(state.pendingAddMediaUrls).toEqual([]);
	 });

  it('routes silent extension captures to the Add Modal instead of queuing immediately', async () => {
  await useDownloadStore.getState().handleExtensionDownload({
   urls: ['https://example.com/downloads/report.pdf'],
   referer: 'https://example.com/page',
   silent: true,
	   filename: 'report.pdf',
	   headers: 'User-Agent: Test',
	   cookies: 'session=secret',
      media: false
	  });

  const state = useDownloadStore.getState();
  expect(state.isAddModalOpen).toBe(true);
  expect(state.pendingAddUrls).toBe('https://example.com/downloads/report.pdf');
  expect(state.pendingAddReferer).toBe('https://example.com/page');
  expect(state.pendingAddFilename).toBe('report.pdf');
	  expect(state.pendingAddHeaders).toBe('User-Agent: Test');
	  expect(state.pendingAddCookies).toBe('session=secret');
    expect(state.pendingAddMediaUrls).toEqual([]);
	 });

  it('keeps each extension handoff context attached to its own URL while the Add Modal is open', async () => {
    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://first.example/file.zip'],
      referer: 'https://first.example/page',
      silent: true,
      filename: 'first.zip',
      headers: 'User-Agent: First Browser',
      cookies: 'first=session',
      media: false
    });
    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://second.example/file.zip'],
      referer: 'https://second.example/page',
      silent: true,
      filename: 'second.zip',
      headers: 'User-Agent: Second Browser',
      cookies: 'second=session',
      media: false
    });

    const state = useDownloadStore.getState();
    expect(state.pendingAddUrls).toBe(
      'https://first.example/file.zip\nhttps://second.example/file.zip'
    );
    expect(state.pendingAddRequestVersion).toBe(2);
    expect(state.pendingAddRequestContexts).toEqual({
      'https://first.example/file.zip': {
        version: 1,
        referer: 'https://first.example/page',
        filename: 'first.zip',
        headers: 'User-Agent: First Browser',
        cookies: 'first=session',
        media: false
      },
      'https://second.example/file.zip': {
        version: 2,
        referer: 'https://second.example/page',
        filename: 'second.zip',
        headers: 'User-Agent: Second Browser',
        cookies: 'second=session',
        media: false
      }
    });
  });

  it('preserves explicit extension media intent for non-allow-listed pages', async () => {
    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://adult.example/watch/123'],
      referer: 'https://adult.example/watch/123',
      silent: false,
      filename: null,
      headers: `Cookie: stale=${'x'.repeat(64 * 1024)}\nUser-Agent: Firefox Test`,
      cookies: `oversized=${'x'.repeat(64 * 1024)}`,
      media: true
    });

    const state = useDownloadStore.getState();
    expect(state.isAddModalOpen).toBe(true);
    expect(state.pendingAddUrls).toBe('https://adult.example/watch/123');
    expect(state.pendingAddMediaUrls).toEqual(['https://adult.example/watch/123']);
    expect(state.pendingAddCookies).toBe('');
    expect(state.pendingAddHeaders).toBe('User-Agent: Firefox Test');
  });

  it('preserves extension cookies for ordinary captured downloads', async () => {
    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://example.com/private.zip'],
      referer: 'https://example.com/downloads',
      silent: true,
      filename: 'private.zip',
      headers: null,
      cookies: 'session=secret',
      media: false
    });

    expect(useDownloadStore.getState().pendingAddCookies).toBe('session=secret');
  });

  it('clears stale request context when the same URL is captured without it later', async () => {
    const url = 'https://example.com/file.zip';
    await useDownloadStore.getState().handleExtensionDownload({
      urls: [url],
      referer: 'https://example.com/private',
      silent: true,
      filename: 'private.zip',
      headers: 'Authorization: secret',
      cookies: 'session=secret',
      media: false
    });
    await useDownloadStore.getState().handleExtensionDownload({
      urls: [url],
      referer: null,
      silent: true,
      filename: null,
      headers: null,
      cookies: null,
      media: false
    });

    expect(useDownloadStore.getState().pendingAddRequestContexts[url]).toEqual({
      version: 2,
      referer: '',
      filename: '',
      headers: '',
      cookies: '',
      media: false
    });
  });

  it('deduplicates forced media URLs and drops stale media intent when opening fresh', async () => {
    useDownloadStore.setState({
      isAddModalOpen: true,
      pendingAddUrls: 'https://adult.example/watch/123',
      pendingAddMediaUrls: ['https://adult.example/watch/123']
    });

    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://adult.example/watch/123'],
      referer: 'https://adult.example/watch/123',
      silent: false,
      filename: null,
      headers: 'User-Agent: Firefox Test',
      cookies: 'session=secret',
      media: true
    });

    expect(useDownloadStore.getState().pendingAddMediaUrls).toEqual([
      'https://adult.example/watch/123'
    ]);

    useDownloadStore.setState({
      isAddModalOpen: false,
      pendingAddMediaUrls: ['https://stale.example/watch']
    });

    await useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://example.com/file.bin'],
      referer: 'https://example.com/page',
      silent: false,
      filename: 'file.bin',
      headers: 'User-Agent: Firefox Test',
      cookies: null,
      media: false
    });

    expect(useDownloadStore.getState().pendingAddMediaUrls).toEqual([]);
  });
});
