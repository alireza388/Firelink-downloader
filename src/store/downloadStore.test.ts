import { beforeEach, describe, expect, it, vi } from 'vitest';
import { initDownloadListener, useDownloadProgressStore } from './downloadStore';
import { useDownloadStore } from './useDownloadStore';
import * as ipc from '../ipc';

vi.mock('../ipc', () => ({
  invokeCommand: vi.fn(),
  listenEvent: vi.fn(),
}));

describe('useDownloadProgressStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useDownloadProgressStore.setState({ progressMap: {} });
  });

  it('prunes terminal progress entries', () => {
    useDownloadProgressStore.getState().updateDownloadProgress('download-1', {
      id: 'download-1',
      fraction: 0.5,
      speed: '1 MB/s',
      eta: '10s',
      size: '2 MB',
      size_is_final: false
    });

    useDownloadProgressStore.getState().clearDownloadProgress('download-1');

    expect(useDownloadProgressStore.getState().progressMap).toEqual({});
  });

  it('shares listener setup across overlapping consumers and tears down after the last release', async () => {
    const unlisten = vi.fn();
    vi.mocked(ipc.listenEvent).mockResolvedValue(unlisten);

    const first = initDownloadListener();
    const second = initDownloadListener();

    expect(ipc.listenEvent).toHaveBeenCalledTimes(3);

    const releaseFirst = await first;
    const releaseSecond = await second;
    releaseFirst();
    expect(unlisten).not.toHaveBeenCalled();

    releaseSecond();
    expect(unlisten).toHaveBeenCalledTimes(3);
  });

  it('ignores late progress and opposite terminal events from an older lifecycle', async () => {
    const handlers: Record<string, (event: any) => void> = {};
    vi.mocked(ipc.listenEvent).mockImplementation((event, handler) => {
      handlers[event] = handler as (event: any) => void;
      return Promise.resolve(vi.fn());
    });
    useDownloadStore.setState({
      downloads: [{
        id: 'terminal',
        url: 'https://example.com/file',
        fileName: 'file.bin',
        status: 'completed',
        category: 'Other',
        dateAdded: ''
      }]
    });

    const release = await initDownloadListener();
    handlers['download-progress']({ payload: {
      id: 'terminal',
      fraction: 0.1,
      speed: '1 MB/s',
      eta: '10s',
      size: '1 MB',
      size_is_final: false
    } });
    handlers['download-state']({ payload: {
      id: 'terminal',
      status: 'failed',
      error: 'stale failure'
    } });

    expect(useDownloadProgressStore.getState().progressMap).toEqual({});
    expect(useDownloadStore.getState().downloads[0].status).toBe('completed');
    release();
  });

  it('clears progress when events arrive after a download row was removed', async () => {
    const handlers: Record<string, (event: any) => void> = {};
    vi.mocked(ipc.listenEvent).mockImplementation((event, handler) => {
      handlers[event] = handler as (event: any) => void;
      return Promise.resolve(vi.fn());
    });
    useDownloadProgressStore.getState().updateDownloadProgress('removed', {
      id: 'removed',
      fraction: 0.8,
      speed: '1 MB/s',
      eta: '2s',
      size: '8 MB',
      size_is_final: false
    });
    useDownloadStore.setState({ downloads: [] });

    const release = await initDownloadListener();
    handlers['download-progress']({ payload: {
      id: 'removed',
      fraction: 0.9,
      speed: '2 MB/s',
      eta: '1s',
      size: '9 MB',
      size_is_final: false
    } });

    expect(useDownloadProgressStore.getState().progressMap).toEqual({});
    release();
  });
});
