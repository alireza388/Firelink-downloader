import { describe, expect, it } from 'vitest';
import {
  canSubmitMetadataRows,
  mediaFormatSelectorForRow,
  metadataSummaryMessage,
  reconcileDownloadRows,
  refreshFailedMetadataRows,
  updateRowIfCurrent,
  type AddDownloadDraftRow
} from './addDownloadMetadata';

const row = (
  overrides: Partial<AddDownloadDraftRow> = {}
): AddDownloadDraftRow => ({
  id: 'row-1',
  sourceUrl: 'https://example.com/file.zip',
  downloadUrl: 'https://example.com/file.zip',
  file: 'file.zip',
  status: 'ready',
  generation: 1,
  isMedia: false,
  ...overrides
});

describe('add download metadata workflow', () => {
  it('preserves rows by normalized source URL and creates only new rows', () => {
    const existing = row({ file: 'server-name.zip' });
    let nextId = 0;
    const rows = reconcileDownloadRows(
      'https://example.com/file.zip\nhttps://example.com/new.zip',
      [existing],
      undefined,
      new Set(),
      () => `new-${nextId++}`
    );

    expect(rows[0]).toBe(existing);
    expect(rows[1]).toMatchObject({
      id: 'new-0',
      status: 'loading',
      file: 'new.zip'
    });
  });

  it('deduplicates normalized URLs and marks malformed or unsupported URLs invalid', () => {
    let nextId = 0;
    const rows = reconcileDownloadRows(
      'https://example.com/a\nhttps://example.com/a\nfile:///tmp/private\nnot-a-url',
      [],
      undefined,
      new Set(),
      () => `row-${nextId++}`
    );

    expect(rows.map(item => item.status)).toEqual(['loading', 'invalid', 'invalid']);
  });

  it('forces explicit extension media fetches through media metadata for any http page', () => {
    const rows = reconcileDownloadRows(
      'https://adult.example/watch/123',
      [],
      undefined,
      new Set(['https://adult.example/watch/123'])
    );

    expect(rows[0]).toMatchObject({
      sourceUrl: 'https://adult.example/watch/123',
      isMedia: true,
      status: 'loading'
    });
  });

  it('upgrades an existing normal row when the user explicitly fetches it as media', () => {
    const existing = row({
      sourceUrl: 'https://adult.example/watch/123',
      downloadUrl: 'https://adult.example/watch/123',
      file: '123',
      status: 'ready',
      generation: 2,
      isMedia: false
    });

    const rows = reconcileDownloadRows(
      'https://adult.example/watch/123',
      [existing],
      undefined,
      new Set(['https://adult.example/watch/123'])
    );

    expect(rows[0]).toMatchObject({
      sourceUrl: 'https://adult.example/watch/123',
      isMedia: true,
      status: 'loading',
      generation: 3,
      formats: undefined,
      selectedFormat: undefined
    });
  });

  it('refreshes only failed metadata and preserves successful format selection', () => {
    const ready = row({
      id: 'ready',
      isMedia: true,
      formats: [{
        name: '1080p MP4',
        selector: 'best',
        ext: 'mp4',
        formatLabel: '1080p',
        detail: '10 MB',
        type: 'Video',
        bytes: 10
      }],
      selectedFormat: 0
    });
    const failed = row({ id: 'failed', status: 'metadata-error', generation: 4 });

    const refreshed = refreshFailedMetadataRows([ready, failed]);

    expect(refreshed[0]).toBe(ready);
    expect(refreshed[1]).toMatchObject({ status: 'loading', generation: 5 });
  });

  it('ignores stale metadata results after generation changes', () => {
    const current = row({ generation: 2, status: 'loading' });
    const updated = updateRowIfCurrent(
      [current],
      current.id,
      current.sourceUrl,
      1,
      value => ({ ...value, status: 'ready' })
    );

    expect(updated[0]).toBe(current);
  });

  it('allows ready and failed rows but blocks loading and invalid rows', () => {
    expect(canSubmitMetadataRows([
      row(),
      row({ id: 'fallback', status: 'metadata-error' })
    ])).toBe(true);
    expect(canSubmitMetadataRows([row({ status: 'loading' })])).toBe(false);
    expect(canSubmitMetadataRows([row({ status: 'invalid' })])).toBe(false);
  });

  it('keeps failed media routing without a format selector', () => {
    const failedMedia = row({
      status: 'metadata-error',
      isMedia: true,
      formats: undefined,
      selectedFormat: undefined
    });

    expect(failedMedia.isMedia).toBe(true);
    expect(mediaFormatSelectorForRow(failedMedia)).toBeUndefined();
  });

  it('reports fallback and invalid states accurately', () => {
    expect(metadataSummaryMessage([
      row(),
      row({ id: 'fallback', status: 'metadata-error' })
    ])).toBe('1 download ready; 1 will use fallback filename and unknown size.');
    expect(metadataSummaryMessage([
      row({ status: 'metadata-error' })
    ])).toContain('can still be added');
    expect(metadataSummaryMessage([
      row({ status: 'invalid' })
    ])).toContain('Correct or remove 1 invalid URL');
  });
});
