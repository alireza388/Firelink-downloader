import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/path', () => ({
  join: vi.fn(async (...parts: string[]) =>
    parts
      .map((part, index) => index === 0 ? part.replace(/[\\/]+$/, '') : part.replace(/^[\\/]+|[\\/]+$/g, ''))
      .join('/')
  )
}));

import {
  downloadLocationEquals,
  DEFAULT_CATEGORY_SUBFOLDERS,
  normalizeCategorySubfolder,
  normalizeDownloadLocationSettings,
  resolveCategoryDestination
} from './downloadLocations';

describe('download locations', () => {
  it('matches backend platform path case semantics', () => {
    expect(downloadLocationEquals('D:\\Downloads', 'Movie.MP4', 'd:/downloads', 'movie.mp4', 'windows')).toBe(true);
    expect(downloadLocationEquals('/Users/Test', 'Movie.MP4', '/users/test', 'movie.mp4', 'macos')).toBe(false);
    expect(downloadLocationEquals('/home/Test', 'Movie.MP4', '/home/test', 'movie.mp4', 'linux')).toBe(false);
  });
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('migrates legacy derived directories without creating overrides', () => {
    const settings = normalizeDownloadLocationSettings({
      defaultDownloadPath: '/Users/test/Downloads',
      downloadDirectories: {
        Movies: '/Users/test/Downloads/Movies',
        Documents: '/Users/test/Downloads/Documents'
      }
    });

    expect(settings.baseDownloadFolder).toBe('/Users/test/Downloads');
    expect(settings.categorySubfolders).toEqual(DEFAULT_CATEGORY_SUBFOLDERS);
    expect(settings.categoryDirectoryOverrides).toEqual({});
  });

  it('preserves legacy custom category directories as overrides', () => {
    const settings = normalizeDownloadLocationSettings({
      defaultDownloadPath: '/Users/test/Downloads',
      downloadDirectories: {
        Video: '/Volumes/Media/Movies'
      }
    });

    expect(settings.categoryDirectoryOverrides.Movies).toBe('/Volumes/Media/Movies');
  });

  it('resolves automatic and overridden category destinations', async () => {
    const automatic = normalizeDownloadLocationSettings({
      baseDownloadFolder: '/Users/test/Downloads',
      categorySubfolders: { Movies: 'Video Files' }
    });
    expect(await resolveCategoryDestination(automatic, 'Movies'))
      .toBe('/Users/test/Downloads/Video Files');

    automatic.categoryDirectoryOverrides.Movies = '/Volumes/Media';
    expect(await resolveCategoryDestination(automatic, 'Movies')).toBe('/Volumes/Media');
  });

  it('keeps category subfolders relative and permits nested folders', () => {
    expect(normalizeCategorySubfolder('../Media/./Movies', 'Movies')).toBe('Media/Movies');
    expect(normalizeCategorySubfolder('C:\\Media\\Movies', 'Movies')).toBe('Media/Movies');
    expect(normalizeCategorySubfolder('../../', 'Movies')).toBe('Movies');
  });
});
