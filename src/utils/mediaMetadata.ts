import { invokeCommand as invoke } from '../ipc';
import type { MediaMetadata } from '../bindings/MediaMetadata';

type FetchMediaMetadataArgs = {
  url: string;
  cookieBrowser: string | null;
  username: string | null;
  password: string | null;
};

const inFlightMediaMetadata = new Map<string, Promise<MediaMetadata>>();

const metadataKey = (args: FetchMediaMetadataArgs) =>
  JSON.stringify([args.url, args.cookieBrowser, args.username, args.password]);

export const fetchMediaMetadataDeduped = (args: FetchMediaMetadataArgs): Promise<MediaMetadata> => {
  const key = metadataKey(args);
  const existing = inFlightMediaMetadata.get(key);
  if (existing) return existing;

  const request = invoke('fetch_media_metadata', args)
    .finally(() => {
      inFlightMediaMetadata.delete(key);
    });
  inFlightMediaMetadata.set(key, request);
  return request;
};
