import { describe, expect, it } from 'vitest';
import {
  convertSpeedValue,
  displayValueFromPresetBase,
  presetBaseFromDisplayValue,
} from './SpeedLimiterView';

describe('SpeedLimiterView speed conversions', () => {
  it('converts KB/s and MB/s using binary units consistently', () => {
    expect(presetBaseFromDisplayValue(1024, 'KB/s')).toBe(1);
    expect(displayValueFromPresetBase(1, 'KB/s')).toBe(1024);
    expect(presetBaseFromDisplayValue(5, 'MB/s')).toBe(5);
    expect(displayValueFromPresetBase(5, 'MB/s')).toBe(5);
  });

  it('round-trips non-MiB-aligned presets through the integer KiB backend value', () => {
    const presetBase = presetBaseFromDisplayValue(1500, 'KB/s');

    expect(displayValueFromPresetBase(presetBase, 'KB/s')).toBe(1500);
    expect(convertSpeedValue(1500, 'KB/s', 'MB/s')).toBe(1500 / 1024);
    expect(convertSpeedValue(convertSpeedValue(1500, 'KB/s', 'MB/s'), 'MB/s', 'KB/s')).toBe(1500);
  });
});
