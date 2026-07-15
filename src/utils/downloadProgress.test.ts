import { describe, expect, it } from 'vitest';
import { formatDownloadBytes, resolveDownloadSizeDisplay } from './downloadProgress';

describe('download progress size display', () => {
  it('formats byte counts using the binary units used by the download engines', () => {
    expect(formatDownloadBytes(0)).toBe('0 B');
    expect(formatDownloadBytes(1.2 * 1024 ** 3)).toBe('1.20 GB');
  });

  it('keeps estimated totals distinguishable from exact totals', () => {
    expect(resolveDownloadSizeDisplay({
      downloadedBytes: 1.2 * 1024 ** 3,
      totalBytes: 2.4 * 1024 ** 3,
      totalIsEstimate: true,
      fallbackSize: 'Unknown'
    })).toEqual({
      downloaded: '1.20 GB',
      total: '2.40 GB',
      totalIsEstimate: true,
      fallback: 'Unknown'
    });
  });
});
