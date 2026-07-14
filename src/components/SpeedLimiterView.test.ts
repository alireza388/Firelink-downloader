import { describe, expect, it } from 'vitest';
import { displayValueFromPresetBase, presetBaseFromDisplayValue } from './SpeedLimiterView';

describe('SpeedLimiterView speed conversions', () => {
  it('converts KB/s and MB/s using binary units consistently', () => {
    expect(presetBaseFromDisplayValue(1024, 'KB/s')).toBe(1);
    expect(displayValueFromPresetBase(1, 'KB/s')).toBe(1024);
    expect(presetBaseFromDisplayValue(5, 'MB/s')).toBe(5);
    expect(displayValueFromPresetBase(5, 'MB/s')).toBe(5);
  });
});
