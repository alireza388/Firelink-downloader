export interface DownloadSizeDisplay {
  downloaded: string | null;
  total: string | null;
  totalIsEstimate: boolean;
  fallback: string;
}

const isUsableByteCount = (value: number | null | undefined): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value >= 0;

export const formatDownloadBytes = (bytes: number): string => {
  if (bytes < 1024) return `${Math.round(bytes)} B`;

  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unitIndex = -1;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  const precision = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(precision)} ${units[unitIndex]}`;
};

export const resolveDownloadSizeDisplay = ({
  downloadedBytes,
  totalBytes,
  totalIsEstimate = false,
  fallbackSize
}: {
  downloadedBytes?: number | null;
  totalBytes?: number | null;
  totalIsEstimate?: boolean;
  fallbackSize?: string | null;
}): DownloadSizeDisplay => ({
  downloaded: isUsableByteCount(downloadedBytes) ? formatDownloadBytes(downloadedBytes) : null,
  total: isUsableByteCount(totalBytes) && totalBytes > 0 ? formatDownloadBytes(totalBytes) : null,
  totalIsEstimate: Boolean(totalIsEstimate && isUsableByteCount(totalBytes) && totalBytes > 0),
  fallback: fallbackSize && fallbackSize !== '-' ? fallbackSize : 'Unknown'
});

export const downloadProgressColorClass = (status: string): string => {
  switch (status) {
    case 'completed':
      return 'download-status-completed';
    case 'paused':
      return 'download-status-paused';
    case 'failed':
      return 'download-status-failed';
    case 'processing':
      return 'download-status-processing';
    case 'queued':
    case 'staged':
      return 'download-status-queued';
    case 'retrying':
      return 'download-status-retrying';
    default:
      return 'download-status-downloading';
  }
};
