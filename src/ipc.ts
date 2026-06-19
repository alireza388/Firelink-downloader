import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { error as logError } from '@tauri-apps/plugin-log';
import { listen as tauriListen, type Event, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadCategory } from './bindings/DownloadCategory';
import type { DownloadItem } from './bindings/DownloadItem';
import type { DownloadProgressEvent } from './bindings/DownloadProgressEvent';
import type { DownloadStateEvent } from './bindings/DownloadStateEvent';
import type { DownloadStatus } from './bindings/DownloadStatus';
import type { ExtensionDownload } from './bindings/ExtensionDownload';
import type { MediaCookieSource } from './bindings/MediaCookieSource';
import type { MediaMetadata } from './bindings/MediaMetadata';
import type { MetadataResponse } from './bindings/MetadataResponse';
import type { EngineStatusItem } from './bindings/EngineStatusItem';
import type { EngineStatusResult } from './bindings/EngineStatusResult';
import type { PostQueueAction } from './bindings/PostQueueAction';
import type { ReleaseCheckOutcome } from './bindings/ReleaseCheckOutcome';

type StartDownloadArgs = {
  id: string;
  url: string;
  destination: string;
  filename: string;
  connections: number | null;
  speedLimit: string | null;
  username: string | null;
  password: string | null;
  headers: string | null;
  checksum: string | null;
  cookies: string | null;
  mirrors: string | null;
  userAgent: string | null;
  maxTries: number | null;
  proxy: string | null;
};

type StartMediaDownloadArgs = {
  id: string;
  url: string;
  destination: string;
  filename: string;
  formatSelector: string | null;
  cookieSource: Exclude<MediaCookieSource, 'none'> | null;
  speedLimit: string | null;
  username: string | null;
  password: string | null;
  headers: string | null;
  proxy: string | null;
  userAgent: string | null;
  maxTries: number | null;
};

type CommandMap = {
  fetch_metadata: {
    args: { url: string; userAgent: string | null; username: string | null; password: string | null };
    result: MetadataResponse;
  };
  fetch_media_metadata: {
    args: { url: string; cookieBrowser: string | null; username: string | null; password: string | null };
    result: MediaMetadata;
  };
 get_engine_status: { args: undefined; result: EngineStatusResult };
 get_aria2_engine_status: { args: undefined; result: EngineStatusItem };
 get_ytdlp_engine_status: { args: undefined; result: EngineStatusItem };
 get_ffmpeg_engine_status: { args: undefined; result: EngineStatusItem };
 get_deno_engine_status: { args: undefined; result: EngineStatusItem };
  open_file: { args: { path: string }; result: void };
  show_in_folder: { args: { path: string }; result: void };
  reveal_in_file_manager: { args: { path: string }; result: void };
  open_downloaded_file: { args: { path: string }; result: void };
  trash_download_assets: { args: { path: string; partialPaths: string[] }; result: void };
  start_download: { args: StartDownloadArgs; result: void };
  start_media_download: { args: StartMediaDownloadArgs; result: void };
  pause_download: { args: { id: string }; result: void };
  resume_download: { args: { id: string }; result: boolean };
  remove_download: { args: { id: string; filepath: string | null }; result: void };
  update_dock_badge: { args: { count: number }; result: void };
  set_prevent_sleep: { args: { prevent: boolean }; result: void };
  perform_system_action: { args: { action: PostQueueAction }; result: void };
  set_concurrent_limit: { args: { limit: number }; result: void };
  set_global_speed_limit: { args: { limit: string | null }; result: void };
  request_automation_permission: { args: undefined; result: void };
  open_automation_settings: { args: undefined; result: void };
  get_free_space: { args: { path: string }; result: string };
  set_keychain_password: { args: { id: string; password: string }; result: void };
  get_keychain_password: { args: { id: string }; result: string };
  delete_keychain_password: { args: { id: string }; result: void };
  check_file_exists: { args: { path: string }; result: boolean };
  delete_file: { args: { path: string }; result: void };
  toggle_tray_icon: { args: { show: boolean }; result: void };
  set_extension_pairing_token: { args: { token: string }; result: void };
  set_extension_frontend_ready: { args: { ready: boolean }; result: void };
  get_system_proxy: { args: undefined; result: string | null };
  get_file_category: { args: { filename: string }; result: DownloadCategory };
  check_for_updates: { args: undefined; result: ReleaseCheckOutcome };
  is_supported_media: { args: { url: string }; result: boolean };
  db_save_settings: { args: { data: string }; result: void };
  db_load_settings: { args: undefined; result: string | null };
  db_get_all_downloads: { args: undefined; result: string[] };
  db_save_download: {
    args: { id: string; status: DownloadStatus; queueId: string; data: string };
    result: void;
  };
  db_delete_download: { args: { id: string }; result: void };
  db_get_all_queues: { args: undefined; result: string[] };
  db_save_queue: { args: { id: string; data: string }; result: void };
  db_delete_queue: { args: { id: string }; result: void };
  create_category_directories: { args: { paths: string[] }; result: void };
  export_logs: { args: { destPath: string }; result: string };
  get_pending_order: { args: undefined; result: string[] };
  enqueue_download: { args: { item: any }; result: string };
  enqueue_many: { args: { items: any[] }; result: void };
  move_in_queue: { args: { id: string; direction: 'up' | 'down' }; result: string[] };
  remove_from_queue: { args: { id: string }; result: boolean };
};

type CommandName = keyof CommandMap;
type CommandArgs<K extends CommandName> = CommandMap[K]['args'];
type CommandResult<K extends CommandName> = CommandMap[K]['result'];

export function invokeCommand<K extends CommandName>(
  command: K,
  ...args: CommandArgs<K> extends undefined ? [] : [args: CommandArgs<K>]
): Promise<CommandResult<K>> {
  return tauriInvoke<CommandResult<K>>(command, args[0]).catch(err => {
    logError(`Invoke command ${command} failed: ${err}`);
    throw err;
  });
}

type EventMap = {
  'schedule-trigger': 'start' | 'stop';
  'download-progress': DownloadProgressEvent;
  'download-state': DownloadStateEvent;
  'download-complete': string;
  'download-failed': string;
  'extension-add-download': ExtensionDownload;
  'extension-downloads-queued': DownloadItem[];
  'deep-link-add-download': string;
};

export function listenEvent<K extends keyof EventMap>(
  event: K,
  handler: EventCallback<EventMap[K]>,
): Promise<UnlistenFn> {
  return tauriListen<EventMap[K]>(event, handler);
}

export type IpcEvent<K extends keyof EventMap> = Event<EventMap[K]>;
