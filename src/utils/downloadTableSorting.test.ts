import { describe, expect, it } from 'vitest';
import type { DownloadItem } from '../bindings/DownloadItem';
import {
  parseDownloadEta,
  parseDownloadSize,
  sortDownloads,
  type DownloadSortConfig
} from './downloadTableSorting';
import { redactDownloadForPersistence } from './downloads';

const item = (id: string, overrides: Partial<DownloadItem> = {}): DownloadItem => ({
  id,
  url: `https://example.test/${id}`,
  fileName: id,
  status: 'queued',
  category: 'Other',
  dateAdded: '2026-07-14T00:00:00.000Z',
  ...overrides
});

const sortedIds = (downloads: DownloadItem[], config: DownloadSortConfig): string[] =>
  sortDownloads(downloads, config).map(download => download.id);

describe('download table sorting', () => {
  it('compares human-readable sizes by bytes instead of their leading number', () => {
    expect(parseDownloadSize('1 MB')).toBe(1024 ** 2);
    expect(sortedIds([
      item('one-mb', { size: '1 MB' }),
      item('900-kb', { size: '900 KB' }),
      item('two-mb', { size: '2 MB' })
    ], { column: 'Size', direction: 'asc' })).toEqual(['900-kb', 'one-mb', 'two-mb']);
  });

  it('supports clock and unit ETA values and keeps unknown values last', () => {
    expect(parseDownloadEta('01:02:03')).toBe(3723);
    expect(parseDownloadEta('2m 5s')).toBe(125);
    expect(sortedIds([
      item('unknown', { eta: '-' }),
      item('long', { eta: '2m' }),
      item('short', { eta: '10s' })
    ], { column: 'ETA', direction: 'asc' })).toEqual(['short', 'long', 'unknown']);
  });

  it('sorts descending on the second click without reverting to an unsorted list', () => {
    const downloads = [item('b', { fileName: 'Beta' }), item('a', { fileName: 'Alpha' })];
    expect(sortedIds(downloads, { column: 'File Name', direction: 'asc' })).toEqual(['a', 'b']);
    expect(sortedIds(downloads, { column: 'File Name', direction: 'desc' })).toEqual(['b', 'a']);
  });

  it('does not persist volatile progress fields', () => {
    const persisted = redactDownloadForPersistence(item('volatile', {
      fraction: 0.75,
      speed: '1 MB/s',
      eta: '10s'
    }));
    expect(persisted.fraction).toBeUndefined();
    expect(persisted.speed).toBeUndefined();
    expect(persisted.eta).toBeUndefined();
  });
});
