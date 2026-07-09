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
  requestContextVersion?: number;
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
  forceMediaUrls: ReadonlySet<string> = new Set(),
  createId: () => string = () => crypto.randomUUID(),
  requestFilenames: Readonly<Record<string, string>> = {},
  requestContextVersions: Readonly<Record<string, number>> = {}
): AddDownloadDraftRow[] => {
  const inputs = parseInputLines(rawText);
  const existing = new Map(currentRows.map(row => [row.sourceUrl, row]));

  return inputs.map(input => {
    const preserved = existing.get(input.sourceUrl);
    if (preserved) {
      const forcedMedia = input.valid && forceMediaUrls.has(input.sourceUrl);
      const requestContextVersion = requestContextVersions[input.sourceUrl];
      const contextChanged = requestContextVersion !== undefined
        && requestContextVersion !== preserved.requestContextVersion;
      if ((forcedMedia && !preserved.isMedia) || contextChanged) {
        return {
          ...preserved,
          status: 'loading',
          generation: preserved.generation + 1,
          requestContextVersion,
          isMedia: preserved.isMedia || forcedMedia,
          formats: preserved.isMedia || forcedMedia ? undefined : preserved.formats,
          selectedFormat: preserved.isMedia || forcedMedia ? undefined : preserved.selectedFormat
        };
      }
      return preserved;
    }

    const requestedFilename = requestFilenames[input.sourceUrl]
      || (inputs.length === 1 ? pendingFilename : undefined);
    const fallback = canonicalizeDownloadFileName(
      requestedFilename || fileNameFromUrl(input.sourceUrl)
    );

    return {
      id: createId(),
      sourceUrl: input.sourceUrl,
      downloadUrl: input.sourceUrl,
      file: fallback,
      status: input.valid ? 'loading' : 'invalid',
      generation: input.valid ? 1 : 0,
      requestContextVersion: requestContextVersions[input.sourceUrl],
      isMedia: input.valid && (forceMediaUrls.has(input.sourceUrl) || isMediaUrl(input.sourceUrl))
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
  && rows.every(row =>
    row.status === 'ready'
    || (!row.isMedia && row.status === 'metadata-error')
  );

export const mediaFormatSelectorForRow = (
  row: AddDownloadDraftRow
): string | undefined => {
  if (!row.isMedia || row.status !== 'ready' || row.selectedFormat === undefined) {
    return undefined;
  }
  return row.formats?.[row.selectedFormat]?.selector;
};

export const mediaFileNameForSelectedFormat = (
  fileName: string,
  row: Pick<AddDownloadDraftRow, 'formats' | 'selectedFormat'>
): string => {
  const selectedFormat = row.selectedFormat === undefined
    ? undefined
    : row.formats?.[row.selectedFormat];
  if (!selectedFormat) return canonicalizeDownloadFileName(fileName);

  const cleanFileName = canonicalizeDownloadFileName(fileName);
  const selectedExt = selectedFormat.ext.replace(/^\.+/, '');
  if (!selectedExt) return cleanFileName;

  const lowerFileName = cleanFileName.toLowerCase();
  if (lowerFileName.endsWith(`.${selectedExt.toLowerCase()}`)) {
    return cleanFileName;
  }

  const lastDot = cleanFileName.lastIndexOf('.');
  const currentExt = lastDot > 0 ? cleanFileName.slice(lastDot + 1).toLowerCase() : '';
  const knownFormatExts = new Set(
    (row.formats || [])
      .map(format => format.ext.replace(/^\.+/, '').toLowerCase())
      .filter(Boolean)
  );
  const baseName = currentExt && knownFormatExts.has(currentExt)
    ? cleanFileName.slice(0, lastDot)
    : cleanFileName;

  return canonicalizeDownloadFileName(`${baseName}.${selectedExt}`);
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
  const failedMedia = rows.filter(row => row.status === 'metadata-error' && row.isMedia).length;
  const ready = rows.filter(row => row.status === 'ready').length;
  if (failedMedia > 0) {
    return `Media metadata is unavailable for ${failedMedia} item${failedMedia === 1 ? '' : 's'}. Refresh metadata before adding.`;
  }
  if (failed === rows.length) {
    return 'Metadata is unavailable. Downloads can still be added using fallback details.';
  }
  if (failed > 0) {
    return `${ready} download${ready === 1 ? '' : 's'} ready; ${failed} will use fallback filename and unknown size.`;
  }
  return `Ready to add ${ready} download${ready === 1 ? '' : 's'}.`;
};
