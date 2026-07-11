import { readText } from '@tauri-apps/plugin-clipboard-manager';
import { extractValidDownloadUrls } from './url';

export const readClipboardDownloadUrls = async (): Promise<string[]> => {
  const clipboardText = await readText();
  return extractValidDownloadUrls(clipboardText);
};
