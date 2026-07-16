import { describe, expect, it } from 'vitest';
import type { DownloadItem } from '../bindings/DownloadItem';
import { redactDownloadForPersistence, resolveDownloadConnections } from './downloads';

const item = (status: DownloadItem['status']): DownloadItem => ({
  id: 'download-1',
  url: 'https://example.com/file.bin',
  fileName: 'file.bin',
  status,
  category: 'Other',
  dateAdded: '2026-07-15T00:00:00.000Z',
  downloadedBytes: 1024,
  totalBytes: 4096,
  totalIsEstimate: false
});

describe('download persistence progress snapshots', () => {
  it('does not write active byte counters on every progress event', () => {
    const persisted = redactDownloadForPersistence(item('downloading'));

    expect(persisted.downloadedBytes).toBeUndefined();
    expect(persisted.totalBytes).toBeUndefined();
    expect(persisted.totalIsEstimate).toBeUndefined();
  });

  it('keeps byte counters for paused snapshots', () => {
    const persisted = redactDownloadForPersistence(item('paused'));

    expect(persisted.downloadedBytes).toBe(1024);
    expect(persisted.totalBytes).toBe(4096);
    expect(persisted.totalIsEstimate).toBe(false);
  });

  it.each(['queued', 'staged', 'retrying', 'processing'] as const)(
    'keeps byte counters for %s snapshots',
    (status) => {
      const persisted = redactDownloadForPersistence(item(status));

      expect(persisted.downloadedBytes).toBe(1024);
      expect(persisted.totalBytes).toBe(4096);
      expect(persisted.totalIsEstimate).toBe(false);
    }
  );
});

describe('download connection resolution', () => {
  it('uses a clamped fallback for legacy rows without a saved value', () => {
    expect(resolveDownloadConnections(undefined, 8)).toBe(8);
    expect(resolveDownloadConnections(undefined, 0)).toBe(1);
    expect(resolveDownloadConnections(undefined, Number.NaN)).toBe(16);
  });

  it('clamps malformed saved values before dispatch', () => {
    expect(resolveDownloadConnections(0, 8)).toBe(1);
    expect(resolveDownloadConnections(17, 8)).toBe(16);
    expect(resolveDownloadConnections(Number.NaN, 8)).toBe(8);
  });
});
