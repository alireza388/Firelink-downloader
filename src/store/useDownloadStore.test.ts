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
      perServerConnections: 16,
      customUserAgent: '',
      maxAutomaticRetries: 3,
      mediaCookieSource: 'none',
      baseDownloadFolder: '~/Downloads',
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
    useDownloadStore.setState({
      downloads: [],
      backendRegisteredIds: new Set(),
      pendingOrder: [],
    });
  });

  it('Start Queue dispatches exactly once for mixed dispatched/undispatched items', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
        { id: '2', url: 'http://test2', fileName: 'f2', destination: '/tmp', status: 'queued', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: false },
      ] as any[],
      backendRegisteredIds: new Set(['1']), // 1 is already registered, so it skips dispatch
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

  it('resumeDownload unregisters ID and re-dispatches if un-resumable', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: '1', url: 'http://test1', fileName: 'f1', destination: '/tmp', status: 'paused', category: 'Other', dateAdded: '', queueId: 'MAIN', hasBeenDispatched: true },
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

  it('retains the UI item when backend removal fails', async () => {
    useDownloadStore.setState({
      downloads: [
        { id: 'active', url: 'https://example.com/file', fileName: 'file', status: 'downloading', category: 'Other', dateAdded: '', queueId: 'main' }
      ] as any[]
    });
    vi.mocked(ipc.invokeCommand).mockRejectedValueOnce(new Error('writer did not stop'));

    await expect(useDownloadStore.getState().removeDownload('active', true))
      .rejects.toThrow('writer did not stop');
    expect(useDownloadStore.getState().downloads.map(download => download.id))
      .toEqual(['active']);
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

  it('preserves extension request headers and cookies for the Add modal', () => {
    useDownloadStore.getState().handleExtensionDownload({
      urls: ['https://example.com/file.bin'],
      referer: 'https://example.com/page',
      silent: false,
      filename: 'file.bin',
      headers: 'X-Test: value',
      cookies: 'session=secret'
    });

    const state = useDownloadStore.getState();
    expect(state.isAddModalOpen).toBe(true);
    expect(state.pendingAddUrls).toBe('https://example.com/file.bin');
    expect(state.pendingAddReferer).toBe('https://example.com/page');
    expect(state.pendingAddFilename).toBe('file.bin');
    expect(state.pendingAddHeaders).toBe('X-Test: value');
    expect(state.pendingAddCookies).toBe('session=secret');
  });
});
