import { open } from '@tauri-apps/plugin-dialog';

export const useDirectoryPicker = () => {
  const pickDirectory = async (defaultPath?: string): Promise<string | null> => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: defaultPath,
      });
      if (selected && typeof selected === 'string') {
        return selected;
      }
      return null;
    } catch (e) {
      console.error('Failed to pick directory:', e);
      return null;
    }
  };

  return { pickDirectory };
};
