import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { error as logError } from '@tauri-apps/plugin-log';
import { listen as tauriListen, type Event, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadCategory } from './bindings/DownloadCategory';
import type { DownloadProgressEvent } from './bindings/DownloadProgressEvent';
import type { DownloadStateEvent } from './bindings/DownloadStateEvent';
import type { ExtensionDownload } from './bindings/ExtensionDownload';
import type { MediaMetadata } from './bindings/MediaMetadata';
import type { MetadataResponse } from './bindings/MetadataResponse';
import type { EngineStatusItem } from './bindings/EngineStatusItem';
import type { PostQueueAction } from './bindings/PostQueueAction';
import type { ReleaseCheckOutcome } from './bindings/ReleaseCheckOutcome';
import type { PairingTokenHydration } from './bindings/PairingTokenHydration';
import type { EnqueueItem } from './bindings/EnqueueItem';
import type { EnqueueAccepted } from './bindings/EnqueueAccepted';

type CommandMap = {
  fetch_metadata: {
    args: { url: string; userAgent: string | null; username: string | null; password: string | null };
    result: MetadataResponse;
  };
  fetch_media_metadata: {
    args: { url: string; cookieBrowser: string | null; username: string | null; password: string | null };
    result: MediaMetadata;
  };
 get_aria2_engine_status: { args: undefined; result: EngineStatusItem };
 get_ytdlp_engine_status: { args: undefined; result: EngineStatusItem };
 get_ffmpeg_engine_status: { args: undefined; result: EngineStatusItem };
 get_deno_engine_status: { args: undefined; result: EngineStatusItem };
  reveal_in_file_manager: { args: { path: string }; result: void };
  open_downloaded_file: { args: { path: string }; result: void };
  pause_download: { args: { id: string }; result: void };
  resume_download: { args: { id: string }; result: boolean };
  remove_download: { args: { id: string; deleteAssets: boolean }; result: void };
  detach_download_for_reconfigure: { args: { id: string }; result: void };
  update_dock_badge: { args: { count: number }; result: void };
  set_prevent_sleep: { args: { prevent: boolean }; result: void };
  perform_system_action: { args: { action: PostQueueAction }; result: void };
  ack_schedule_trigger: { args: { action: 'start' | 'stop'; key: string }; result: void };
  set_concurrent_limit: { args: { limit: number }; result: void };
  set_global_speed_limit: { args: { limit: string | null }; result: void };
  request_automation_permission: { args: undefined; result: void };
  check_automation_permission: { args: undefined; result: void };
  open_automation_settings: { args: undefined; result: void };
  get_free_space: { args: { path: string }; result: string };
  set_keychain_password: { args: { id: string; password: string }; result: void };
  get_keychain_password: { args: { id: string }; result: string };
  delete_keychain_password: { args: { id: string }; result: void };
  check_file_exists: { args: { path: string }; result: boolean };
  toggle_tray_icon: { args: { show: boolean }; result: void };
  set_extension_pairing_token: { args: { token: string }; result: void };
  get_extension_server_port: { args: undefined; result: number | null };
  hydrate_extension_pairing_token: { args: undefined; result: PairingTokenHydration };
  acknowledge_pairing_token_change: { args: undefined; result: void };
  set_extension_frontend_ready: { args: { ready: boolean }; result: void };
  get_system_proxy: { args: undefined; result: string | null };
  get_file_category: { args: { filename: string }; result: DownloadCategory };
  check_for_updates: { args: undefined; result: ReleaseCheckOutcome };
  get_supported_media_domains: { args: undefined; result: string[] };
  db_save_settings: { args: { data: string }; result: void };
  db_load_settings: { args: undefined; result: string | null };
  db_get_all_downloads: { args: undefined; result: string[] };
  db_replace_downloads: { args: { data: string }; result: void };
  db_get_all_queues: { args: undefined; result: string[] };
  db_replace_queues: { args: { data: string }; result: void };
  create_category_directories: {
    args: { baseFolder: string; subfolders: Record<string, string> };
    result: void;
  };
  export_logs: { args: { destPath: string }; result: string };
  read_logs: { args: { limit: number }; result: string[] };
  toggle_log_pause: { args: { pause: boolean }; result: void };
  is_log_paused: { args: undefined; result: boolean };
  get_pending_order: { args: { queueId: string | null }; result: string[] };
  enqueue_download: { args: { item: EnqueueItem }; result: EnqueueAccepted };
  enqueue_many: { args: { items: EnqueueItem[] }; result: import('./bindings/EnqueueResult').EnqueueResult[] };
  move_in_queue: { args: { id: string; queueId: string; direction: 'up' | 'down' }; result: string[] };
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
    void logError(`Invoke command ${command} failed: ${err}`).catch(() => undefined);
    throw err;
  });
}

type EventMap = {
  'schedule-trigger': { action: 'start' | 'stop'; key: string };
  'download-progress': DownloadProgressEvent;
  'download-state': DownloadStateEvent;
  'download-complete': string;
  'download-failed': string;
  'extension-add-download': ExtensionDownload;
  'deep-link-add-download': string;
  'tray-action': 'pause-all' | 'resume-all';
};

export function listenEvent<K extends keyof EventMap>(
  event: K,
  handler: EventCallback<EventMap[K]>,
): Promise<UnlistenFn> {
  return tauriListen<EventMap[K]>(event, handler);
}

export type IpcEvent<K extends keyof EventMap> = Event<EventMap[K]>;
