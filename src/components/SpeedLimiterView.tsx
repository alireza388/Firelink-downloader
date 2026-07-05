import { useEffect, useMemo, useState } from 'react';
import { Gauge, Plus, Save, X, Zap } from 'lucide-react';
import { useSettingsStore } from '../store/useSettingsStore';
import { WindowDragRegion } from './WindowDragRegion';
import { useToast } from '../contexts/ToastContext';

type SpeedUnit = 'KB/s' | 'MB/s';

const MAX_LIMIT_KIB = 10_485_760;
const MAX_LIMIT_MB = 10240;

function parseLimit(limit: string, fallback: number): { value: number; unit: SpeedUnit } {
  const match = limit.trim().match(/^(\d+(?:\.\d+)?)\s*([km]?)b?(?:\/s)?$/i);
  const valueKiB = match
    ? Math.max(1, Math.round(Number(match[1]) * (match[2].toLowerCase() === 'm' ? 1024 : 1)))
    : fallback;

  return valueKiB >= 1024 && valueKiB % 1024 === 0
    ? { value: valueKiB / 1024, unit: 'MB/s' }
    : { value: valueKiB, unit: 'KB/s' };
}

function sanitizePresetValues(values: number[]): number[] {
  const cleaned = values
    .map(value => Number(value))
    .filter(value => Number.isFinite(value) && value > 0)
    .map(value => Math.min(MAX_LIMIT_MB, Math.round(value * 100) / 100));

  return Array.from(new Set(cleaned)).sort((a, b) => a - b);
}

function presetBaseFromDisplayValue(value: number, unit: SpeedUnit): number {
  return unit === 'MB/s' ? value : value / 1000;
}

function displayValueFromPresetBase(value: number, unit: SpeedUnit): number {
  return unit === 'MB/s' ? value : Math.round(value * 1000);
}

function formatPresetValue(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2).replace(/\.?0+$/, '');
}

export default function SpeedLimiterView() {
  const globalSpeedLimit = useSettingsStore(state => state.globalSpeedLimit);
  const lastCustomSpeedLimitKiB = useSettingsStore(state => state.lastCustomSpeedLimitKiB);
  const speedLimitPresetValues = useSettingsStore(state => state.speedLimitPresetValues);
  const setGlobalSpeedLimit = useSettingsStore(state => state.setGlobalSpeedLimit);
  const setLastCustomSpeedLimitKiB = useSettingsStore(state => state.setLastCustomSpeedLimitKiB);
  const setSpeedLimitPresetValues = useSettingsStore(state => state.setSpeedLimitPresetValues);
  const initial = parseLimit(globalSpeedLimit, lastCustomSpeedLimitKiB);
  const [enabled, setEnabled] = useState(Boolean(globalSpeedLimit));
  const [value, setValue] = useState(initial.value);
  const [unit, setUnit] = useState<SpeedUnit>(initial.unit);
  const [customPresetValue, setCustomPresetValue] = useState(initial.value);
  const { addToast } = useToast();
  const presetValues = useMemo(
    () => sanitizePresetValues(speedLimitPresetValues),
    [speedLimitPresetValues]
  );

  useEffect(() => {
    const parsed = parseLimit(globalSpeedLimit, lastCustomSpeedLimitKiB);
    setEnabled(Boolean(globalSpeedLimit));
    setValue(parsed.value);
    setUnit(parsed.unit);
    setCustomPresetValue(parsed.value);
  }, [globalSpeedLimit, lastCustomSpeedLimitKiB]);


  const save = () => {
    const numericValue = Math.max(1, Math.min(Number(value) || 1, unit === 'MB/s' ? 10240 : MAX_LIMIT_KIB));
    const valueKiB = Math.min(MAX_LIMIT_KIB, Math.round(unit === 'MB/s' ? numericValue * 1024 : numericValue));
    setLastCustomSpeedLimitKiB(valueKiB);
    setGlobalSpeedLimit(enabled ? `${valueKiB}K` : '');
    addToast({
      message: enabled ? `Global limit saved at ${numericValue} ${unit}` : 'Global speed limit disabled',
      variant: 'success'
    });
  };

  const preset = (presetValue: number) => {
    setEnabled(true);
    setValue(displayValueFromPresetBase(presetValue, unit));
  };

  const applyCustomPreset = () => {
    const numericValue = Math.max(1, Math.min(Number(customPresetValue) || 1, unit === 'MB/s' ? MAX_LIMIT_MB : MAX_LIMIT_KIB));
    const presetBaseValue = Math.min(MAX_LIMIT_MB, presetBaseFromDisplayValue(numericValue, unit));
    const nextPresets = sanitizePresetValues([...presetValues, presetBaseValue]);
    const alreadyExists = nextPresets.length === presetValues.length;
    setSpeedLimitPresetValues(nextPresets);
    setEnabled(true);
    setValue(numericValue);
    addToast({
      message: alreadyExists
        ? `${formatPresetValue(numericValue)} ${unit} is already in quick presets`
        : `Added ${formatPresetValue(numericValue)} ${unit} quick preset`,
      variant: alreadyExists ? 'info' : 'success'
    });
  };

  const removePreset = (presetValue: number) => {
    const displayValue = displayValueFromPresetBase(presetValue, unit);
    const nextPresets = presetValues.filter(value => value !== presetValue);
    setSpeedLimitPresetValues(nextPresets);
    addToast({
      message: `Removed ${formatPresetValue(displayValue)} ${unit} quick preset`,
      variant: 'info'
    });
  };

  return (
    <div className="flex-1 flex h-full flex-col overflow-hidden bg-main-bg">
      <WindowDragRegion />

      <div className="flex items-center gap-3 border-b border-border-color px-6 pb-4">
        <div className="flex items-center gap-3 text-[17px] font-semibold tracking-tight text-text-primary select-none">
          <button
            onClick={() => setEnabled(!enabled)}
            className={`relative inline-flex h-5 w-9 cursor-pointer items-center rounded-full transition-colors duration-200 ease-in-out focus:outline-none ${enabled ? 'bg-accent' : 'bg-item-hover'}`}
            aria-checked={enabled}
            role="switch"
          >
            <span
              className={`inline-block h-4 w-4 transform rounded-full bg-white transition duration-200 ease-in-out ${enabled ? 'translate-x-4' : 'translate-x-1'}`}
            />
          </button>
          Speed Limiter
        </div>
        <span className={`rounded-full px-2.5 py-1 text-[11px] font-semibold ${
          enabled ? 'bg-accent/15 text-accent' : 'bg-item-hover text-text-muted'
        }`}>
          {enabled ? `${value} ${unit}` : 'Unlimited'}
        </span>
        <button onClick={save} className="app-button app-button-primary ml-auto px-3 text-[11px]">
          <Save size={14} /> Save Limit
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <section className={`app-card max-w-[760px] p-5 ${enabled ? '' : 'opacity-55'}`}>
          <div className="mb-2 flex items-center gap-2 font-semibold text-text-primary">
            <Gauge size={18} className="text-accent" /> Global Speed Limit
          </div>
          <p className="max-w-2xl text-[12px] leading-relaxed text-text-muted">
            Applies to new and active aria2 transfers and yt-dlp media downloads. Per-download limits still take precedence.
          </p>

          <div className="mt-6 flex items-center gap-3">
            <input
              type="number"
              min="1"
              value={value}
              disabled={!enabled}
              onChange={event => setValue(Math.max(1, Number(event.target.value) || 1))}
              className="app-control w-28 px-3 py-2 text-right font-mono"
            />
            <div className="flex rounded-md border border-border-modal bg-bg-input p-1">
              {(['KB/s', 'MB/s'] as SpeedUnit[]).map(option => (
                <button
                  key={option}
                  type="button"
                  disabled={!enabled}
                  onClick={() => setUnit(option)}
                  className={`rounded px-3 py-1.5 text-[12px] font-medium ${
                    unit === option ? 'bg-accent text-white' : 'text-text-secondary hover:bg-item-hover'
                  }`}
                >
                  {option}
                </button>
              ))}
            </div>
          </div>

          <div className="my-6 border-t border-border-color" />
          <div className="mb-3 flex items-center gap-2 text-[12px] font-medium text-text-secondary">
            <Zap size={14} /> Quick Presets
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {presetValues.map(presetValue => {
              const displayValue = displayValueFromPresetBase(presetValue, unit);
              return (
                <div
                  key={presetValue}
                  className="group flex h-8 min-w-[92px] items-center overflow-hidden rounded-md border border-border-modal bg-bg-input text-[12px] text-text-primary transition-colors hover:bg-item-hover"
                >
                  <button
                    type="button"
                    disabled={!enabled}
                    onClick={() => preset(presetValue)}
                    className="h-full flex-1 px-3 text-left disabled:opacity-50"
                  >
                    {formatPresetValue(displayValue)} {unit}
                  </button>
                  <button
                    type="button"
                    disabled={!enabled}
                    onClick={() => removePreset(presetValue)}
                    className="flex h-full w-7 items-center justify-center border-l border-border-modal text-text-muted transition-colors hover:bg-red-500/10 hover:text-red-400 focus-visible:bg-red-500/10 focus-visible:text-red-400 disabled:opacity-50"
                    title={`Remove ${formatPresetValue(displayValue)} ${unit} preset`}
                    aria-label={`Remove ${formatPresetValue(displayValue)} ${unit} preset`}
                  >
                    <X size={12} />
                  </button>
                </div>
              );
            })}
            <div className="ml-1 flex h-8 items-center gap-1.5 rounded-md border border-border-modal bg-bg-input px-2">
              <input
                type="number"
                min="1"
                value={customPresetValue}
                disabled={!enabled}
                onChange={event => setCustomPresetValue(Math.max(1, Number(event.target.value) || 1))}
                className="w-12 bg-transparent text-right font-mono text-[12px] text-text-primary outline-none disabled:opacity-50"
                aria-label={`Custom preset in ${unit}`}
              />
              <span className="text-[11px] text-text-muted">{unit}</span>
              <button
                type="button"
                disabled={!enabled}
                onClick={applyCustomPreset}
                className="app-icon-button h-6 w-6 disabled:opacity-50"
                title="Add quick preset"
                aria-label="Add quick preset"
              >
                <Plus size={14} />
              </button>
            </div>
          </div>
        </section>
      </div>

    </div>
  );
}
