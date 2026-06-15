import type { DownloadCategory } from '../bindings/DownloadCategory';
export type { DownloadCategory } from '../bindings/DownloadCategory';

const MEDIA_DOMAINS = [
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
  'fb.watch'
];

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
