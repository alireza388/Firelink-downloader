import { beforeEach, describe, expect, it } from 'vitest';
import { useDownloadProgressStore } from './downloadStore';

describe('useDownloadProgressStore', () => {
  beforeEach(() => {
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
});
