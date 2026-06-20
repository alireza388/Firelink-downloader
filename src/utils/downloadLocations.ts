import { join, homeDir } from '@tauri-apps/api/path';
import type { DownloadCategory } from '../bindings/DownloadCategory';

export const expandTilde = async (path: string): Promise<string> => {
  if (path.startsWith('~/') || path.startsWith('~\\')) {
    const home = await homeDir();
    return join(home, path.slice(2));
  }
  if (path === '~') {
    return homeDir();
  }
  return path;
};

export const DOWNLOAD_CATEGORIES: DownloadCategory[] = [
  'Musics',
  'Movies',
  'Compressed',
  'Documents',
  'Pictures',
  'Applications',
  'Other'
];

export const DEFAULT_CATEGORY_SUBFOLDERS: Record<DownloadCategory, string> = {
  Musics: 'Musics',
  Movies: 'Movies',
  Compressed: 'Compressed',
  Documents: 'Documents',
  Pictures: 'Pictures',
  Applications: 'Applications',
  Other: 'Other'
};

export interface DownloadLocationSettings {
  baseDownloadFolder: string;
  categorySubfolders: Record<string, string>;
  categoryDirectoryOverrides: Record<string, string>;
}

interface LegacyDownloadLocationSettings {
  baseDownloadFolder?: unknown;
  categorySubfolders?: unknown;
  categoryDirectoryOverrides?: unknown;
  defaultDownloadPath?: unknown;
  downloadDirectories?: unknown;
}

const stringRecord = (value: unknown): Record<string, string> => {
  if (!value || typeof value !== 'object') return {};
  return Object.fromEntries(
    Object.entries(value)
      .filter((entry): entry is [string, string] => typeof entry[1] === 'string')
      .map(([key, path]) => [key, path.trim()])
  );
};

const normalizedForComparison = (value: string): string =>
  value.replace(/\\/g, '/').replace(/\/+$/, '');

const legacyDerivedPath = (base: string, subfolder: string): string =>
  `${normalizedForComparison(base)}/${subfolder.replace(/^[\\/]+|[\\/]+$/g, '')}`;

export const normalizeCategorySubfolder = (
  value: string,
  fallback: string
): string => {
  const parts = value
    .trim()
    .replace(/\\/g, '/')
    .split('/')
    .filter(part => part && part !== '.' && part !== '..' && !part.endsWith(':'));
  return parts.join('/') || fallback;
};

export const normalizeDownloadLocationSettings = (
  value: LegacyDownloadLocationSettings
): DownloadLocationSettings => {
  const baseDownloadFolder =
    (typeof value.baseDownloadFolder === 'string' && value.baseDownloadFolder.trim()) ||
    (typeof value.defaultDownloadPath === 'string' && value.defaultDownloadPath.trim()) ||
    '~/Downloads';
  const persistedSubfolders = stringRecord(value.categorySubfolders);
  const categorySubfolders = Object.fromEntries(
    DOWNLOAD_CATEGORIES.map(category => [
      category,
      normalizeCategorySubfolder(
        persistedSubfolders[category] || '',
        DEFAULT_CATEGORY_SUBFOLDERS[category]
      )
    ])
  );
  const categoryDirectoryOverrides = stringRecord(value.categoryDirectoryOverrides);
  const legacyDirectories = stringRecord(value.downloadDirectories);
  const legacyAliases: Record<DownloadCategory, string> = {
    Musics: 'Audio',
    Movies: 'Video',
    Compressed: 'Archives',
    Documents: 'Documents',
    Pictures: 'Images',
    Applications: 'Apps',
    Other: 'Other'
  };

  for (const category of DOWNLOAD_CATEGORIES) {
    const legacyDirectory =
      legacyDirectories[category] || legacyDirectories[legacyAliases[category]];
    if (categoryDirectoryOverrides[category] || !legacyDirectory) continue;
    const expected = legacyDerivedPath(baseDownloadFolder, categorySubfolders[category]);
    if (normalizedForComparison(legacyDirectory) !== expected) {
      categoryDirectoryOverrides[category] = legacyDirectory;
    }
  }

  return {
    baseDownloadFolder,
    categorySubfolders,
    categoryDirectoryOverrides
  };
};

export const resolveCategoryDestination = async (
  settings: DownloadLocationSettings,
  category: DownloadCategory
): Promise<string> => {
  const override = settings.categoryDirectoryOverrides[category]?.trim();
  if (override) return expandTilde(override);

  const base = settings.baseDownloadFolder.trim() || '~/Downloads';
  const expandedBase = await expandTilde(base);
  const subfolder =
    normalizeCategorySubfolder(
      settings.categorySubfolders[category] || '',
      DEFAULT_CATEGORY_SUBFOLDERS[category]
    );
  return join(expandedBase, subfolder);
};

export const resolveDownloadFilePath = async (
  destination: string,
  fileName: string
): Promise<string> => {
  const expandedDest = await expandTilde(destination);
  return join(expandedDest, fileName);
};
