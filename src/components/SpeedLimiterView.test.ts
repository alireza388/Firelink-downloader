import { describe, expect, it } from 'vitest';
import {
  convertSpeedValue,
  clampSpeedDisplayValue,
  displayValueFromPresetBase,
  formatSpeedLimitForStorage,
  formatPresetValue,
  parseLimit,
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

  it('preserves the selected unit when saving a non-MiB-aligned MB/s value', () => {
    const storedLimit = formatSpeedLimitForStorage(1.5, 'MB/s');

    expect(storedLimit).toBe('1.5M');
    expect(parseLimit(storedLimit, 1024)).toEqual({ value: 1.5, unit: 'MB/s' });
  });

  it('preserves an explicitly selected KB/s unit at MiB boundaries', () => {
    const storedLimit = formatSpeedLimitForStorage(1024, 'KB/s');

    expect(storedLimit).toBe('1024K');
    expect(parseLimit(storedLimit, 1)).toEqual({ value: 1024, unit: 'KB/s' });
  });

  it('restores the persisted unit while the limiter is disabled', () => {
    expect(parseLimit('', 1536, 'MB/s')).toEqual({ value: 1.5, unit: 'MB/s' });
    expect(parseLimit('', 1536, 'KB/s')).toEqual({ value: 1536, unit: 'KB/s' });
  });

  it('allows sub-one MB/s values while enforcing the 1 KiB backend minimum', () => {
    expect(clampSpeedDisplayValue(0.5, 'MB/s')).toBe(0.5);
    expect(clampSpeedDisplayValue(0, 'MB/s')).toBe(1 / 1024);
    expect(clampSpeedDisplayValue(0, 'KB/s')).toBe(1);
  });

  it('shows the exact stored preset value instead of a rounded label', () => {
    expect(formatPresetValue(1.46484375)).toBe('1.46484375');
  });
});
