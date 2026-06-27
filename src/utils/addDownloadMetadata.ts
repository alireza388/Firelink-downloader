import {
  canonicalizeDownloadFileName,
  fileNameFromUrl,
  isMediaUrl
} from './downloads';

export type MetadataStatus = 'loading' | 'ready' | 'metadata-error' | 'invalid';

export interface AddMediaFormat {
  name: string;
  selector: string;
  ext: string;
  formatLabel: string;
  detail: string;
  type: string;
  bytes: number;
  isApproximate?: boolean;
}

export interface AddDownloadDraftRow {
  id: string;
  sourceUrl: string;
  downloadUrl: string;
  file: string;
  size?: string;
  sizeBytes?: number;
  status: MetadataStatus;
  generation: number;
  isMedia: boolean;
  resumable?: boolean;
  formats?: AddMediaFormat[];
  selectedFormat?: number;
}

const ALLOWED_SCHEMES = new Set(['http:', 'https:', 'ftp:', 'sftp:']);

type ParsedInput = {
  identity: string;
  sourceUrl: string;
  valid: boolean;
};

const parseInputLines = (rawText: string): ParsedInput[] => {
  const seen = new Set<string>();
  const parsed: ParsedInput[] = [];

  for (const rawLine of rawText.split('\n')) {
    const line = rawLine.trim();
    if (!line) continue;

    let sourceUrl = line;
    let valid = false;
    try {
      const url = new URL(line);
      valid = ALLOWED_SCHEMES.has(url.protocol);
      if (valid) sourceUrl = url.href;
    } catch {
      valid = false;
    }

    const identity = valid ? sourceUrl : `invalid:${line}`;
    if (seen.has(identity)) continue;
    seen.add(identity);
    parsed.push({ identity, sourceUrl, valid });
  }

  return parsed;
};

export const reconcileDownloadRows = (
  rawText: string,
  currentRows: AddDownloadDraftRow[],
  pendingFilename?: string,
  createId: () => string = () => crypto.randomUUID()
): AddDownloadDraftRow[] => {
  const inputs = parseInputLines(rawText);
  const existing = new Map(currentRows.map(row => [row.sourceUrl, row]));

  return inputs.map(input => {
    const preserved = existing.get(input.sourceUrl);
    if (preserved) return preserved;

    const fallback = canonicalizeDownloadFileName(
      inputs.length === 1 && pendingFilename
        ? pendingFilename
        : fileNameFromUrl(input.sourceUrl)
    );

    return {
      id: createId(),
      sourceUrl: input.sourceUrl,
      downloadUrl: input.sourceUrl,
      file: fallback,
      status: input.valid ? 'loading' : 'invalid',
      generation: input.valid ? 1 : 0,
      isMedia: input.valid && isMediaUrl(input.sourceUrl)
    };
  });
};

export const updateRowIfCurrent = (
  rows: AddDownloadDraftRow[],
  id: string,
  sourceUrl: string,
  generation: number,
  update: (row: AddDownloadDraftRow) => AddDownloadDraftRow
): AddDownloadDraftRow[] => rows.map(row =>
  row.id === id && row.sourceUrl === sourceUrl && row.generation === generation
    ? update(row)
    : row
);

export const refreshFailedMetadataRows = (
  rows: AddDownloadDraftRow[]
): AddDownloadDraftRow[] => rows.map(row =>
  row.status === 'metadata-error'
    ? {
        ...row,
        status: 'loading',
        generation: row.generation + 1
      }
    : row
);

export const canSubmitMetadataRows = (rows: AddDownloadDraftRow[]): boolean =>
  rows.length > 0
  && rows.every(row => row.status === 'ready' || row.status === 'metadata-error');

export const mediaFormatSelectorForRow = (
  row: AddDownloadDraftRow
): string | undefined => {
  if (!row.isMedia || row.status !== 'ready' || row.selectedFormat === undefined) {
    return undefined;
  }
  return row.formats?.[row.selectedFormat]?.selector;
};

export const metadataSummaryMessage = (rows: AddDownloadDraftRow[]): string => {
  if (rows.length === 0) return 'Paste one or more links.';

  const invalid = rows.filter(row => row.status === 'invalid').length;
  if (invalid > 0) {
    return `Correct or remove ${invalid} invalid URL${invalid === 1 ? '' : 's'} before continuing.`;
  }

  const loading = rows.filter(row => row.status === 'loading').length;
  if (loading > 0) {
    return `Waiting for metadata for ${loading} download${loading === 1 ? '' : 's'}.`;
  }

  const failed = rows.filter(row => row.status === 'metadata-error').length;
  const ready = rows.filter(row => row.status === 'ready').length;
  if (failed === rows.length) {
    return 'Metadata is unavailable. Downloads can still be added using fallback details.';
  }
  if (failed > 0) {
    return `${ready} download${ready === 1 ? '' : 's'} ready; ${failed} will use fallback filename and unknown size.`;
  }
  return `Ready to add ${ready} download${ready === 1 ? '' : 's'}.`;
};
