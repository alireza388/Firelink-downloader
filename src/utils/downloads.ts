import type { DownloadCategory } from '../bindings/DownloadCategory';
import type { DownloadStatus } from '../bindings/DownloadStatus';
import type { DownloadItem } from '../bindings/DownloadItem';
export type { DownloadCategory } from '../bindings/DownloadCategory';

import { invokeCommand as invoke } from '../ipc';

let MEDIA_DOMAINS = [
  'youtube.com',
  'youtu.be',
  'twitter.com',
  'x.com',
  'twitch.tv',
  'vimeo.com',
  'instagram.com',
  'tiktok.com',
  'reddit.com',
  'v.redd.it',
  'soundcloud.com',
  'facebook.com',
  'fb.watch',
  'pornhub.com',
  'redtube.com',
  'xhamster.com',
  'xnxx.com',
  'xvideos.com'
];

const ACTIVE_DOWNLOAD_STATUSES: ReadonlySet<DownloadStatus> = new Set([
  'queued',
  'downloading',
  'processing',
  'retrying',
]);

export const isActiveDownloadStatus = (status: DownloadStatus): boolean =>
  ACTIVE_DOWNLOAD_STATUSES.has(status);

/** Transfer states that consume a worker/permit. Queued is intentionally excluded. */
export const isTransferActiveStatus = (status: DownloadStatus): boolean =>
  status === 'downloading' || status === 'processing' || status === 'retrying';

export const normalizeSpeedLimitForBackend = (value?: string | null): string | null => {
  const trimmed = value?.trim();
  if (!trimmed) return null;

  const match = trimmed.match(/^(\d+(?:\.\d+)?)\s*([kmgt]?)i?b?(?:\/s)?$/i);
  if (!match) return null;

  const amount = Number(match[1]);
  if (!Number.isFinite(amount) || amount <= 0) return null;

  const unit = match[2].toUpperCase();
  return unit ? `${amount}${unit}` : `${amount}K`;
};

export const initMediaDomains = async () => {
  try {
    const domains = await invoke('get_supported_media_domains');
    if (domains && domains.length > 0) {
      MEDIA_DOMAINS = domains;
    }
  } catch (e) {
    console.error('Failed to init media domains:', e);
  }
};

export const categoryForFileName = (fileName: string): DownloadCategory => {
  const ext = fileName.split('.').pop()?.toLowerCase() || '';
  if (['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'mpeg', 'mpg', '3gp', 'ts', 'vob'].includes(ext)) return 'Movies';
  if (['mp3', 'wav', 'aac', 'flac', 'ogg', 'm4a', 'wma', 'alac', 'ape', 'mid', 'midi'].includes(ext)) return 'Musics';
  if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'rtf', 'csv', 'md', 'epub', 'mobi', 'azw3'].includes(ext)) return 'Documents';
  if (['exe', 'msi', 'bat', 'cmd', 'app', 'dmg', 'pkg', 'apk', 'appx', 'deb', 'rpm', 'appimage', 'run', 'sh', 'bin', 'jar'].includes(ext)) return 'Applications';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'tiff', 'svg', 'ico', 'heic', 'raw', 'psd', 'ai'].includes(ext)) return 'Pictures';
  if (['zip', 'rar', '7z', 'tar', 'gz', 'xz', 'bz2', 'lz', 'lzma', 'zst', 'iso', 'cab', 'tgz', 'tbz', 'z', 'sit', 'sitx'].includes(ext)) return 'Compressed';
  return 'Other';
};

export const fileNameFromUrl = (rawUrl: string): string => {
  try {
    const url = new URL(rawUrl);
    const pathName = url.pathname.split('/').filter(Boolean).pop();
    if (pathName) {
      const decoded = decodeURIComponent(pathName).trim();
      if (decoded && decoded !== '.' && decoded !== '..') {
        return decoded.replace(/[\/\\?%*:|"<>]/g, '-');
      }
    }
  } catch {
    // Fall through to the stable generic name.
  }
  return 'download';
};

export const canonicalizeDownloadFileName = (fileName: string): string => {
  const leaf = fileName.replace(/\\/g, '/').split('/').pop() || 'download';
  const sanitized = leaf
    .replace(/[\u0000-\u001f<>:"/\\|?*]/g, '-')
    .trim()
    .replace(/[. ]+$/g, '');
  return sanitized && sanitized !== '.' && sanitized !== '..' ? sanitized : 'download';
};

export const isMediaUrl = (rawUrl: string): boolean => {
  try {
    const url = new URL(rawUrl);
    return MEDIA_DOMAINS.some(domain =>
      url.hostname === domain || url.hostname.endsWith(`.${domain}`)
    );
  } catch {
    return false;
  }
};

/**
 * Fields that may carry secrets and therefore must never reach the persisted
 * `download_queue` document. These are supplied in-memory for the active
 * session (see `enqueue_download` payloads) but are stripped at the
 * persistence boundary so the user-data database contains no plaintext credentials.
 */
const DOWNLOAD_SECRET_FIELDS = ['password', 'cookies', 'headers'] as const;
const VOLATILE_PROGRESS_STATUSES = new Set([
  'ready',
  'staged',
  'queued',
  'downloading',
  'processing',
  'retrying'
]);

/**
 * Returns a shallow copy of `item` with secret fields removed. Volatile
 * progress fields (`fraction`, `speed`, `eta`) are also dropped as in the
 * existing persistence path. Numeric byte totals remain for paused, failed,
 * and completed rows so those snapshots keep their accurate Size-column
 * display after restart; active-transfer counters stay in memory to avoid a
 * database write for every progress tick.
 *
 * Note: standard persistence intentionally retains `url` because it is the
 * download source. The backend applies a stricter portable-mode policy: URL
 * userinfo, query, and fragment components are removed before portable data
 * is written, and affected active records are not auto-resumed.
 */
export const redactDownloadForPersistence = (item: DownloadItem): DownloadItem => {
  const copy: DownloadItem = { ...item };
  delete copy.fraction;
  delete copy.speed;
  delete copy.eta;
  if (VOLATILE_PROGRESS_STATUSES.has(item.status)) {
    delete copy.downloadedBytes;
    delete copy.totalBytes;
    delete copy.totalIsEstimate;
  }
  for (const field of DOWNLOAD_SECRET_FIELDS) {
    delete copy[field];
  }
  return copy;
};
