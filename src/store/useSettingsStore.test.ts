import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from './useSettingsStore';
import * as ipc from '../ipc';

vi.mock('../ipc', () => ({
  invokeCommand: vi.fn()
}));

vi.mock('../utils/logger', () => ({
  info: vi.fn()
}));

describe('useSettingsStore global speed limit persistence', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({ globalSpeedLimit: '2M' });
  });

  it('keeps the saved value when the backend rejects a limit change', async () => {
    vi.mocked(ipc.invokeCommand).mockRejectedValueOnce(new Error('aria2 unavailable'));

    await expect(useSettingsStore.getState().setGlobalSpeedLimit('3M')).rejects.toThrow('aria2 unavailable');

    expect(useSettingsStore.getState().globalSpeedLimit).toBe('2M');
    expect(ipc.invokeCommand).toHaveBeenCalledWith('set_global_speed_limit', { limit: '3M' });
  });
});
