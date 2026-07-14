import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/plugin-log', () => ({
  attachLogger: vi.fn(),
  debug: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  trace: vi.fn(),
  warn: vi.fn()
}));

import { setLogPaused, setLogStreamActive } from './logger';

describe('logger stream transitions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('serializes enable and disable transitions', async () => {
    let releaseFirst!: () => void;
    invoke
      .mockImplementationOnce(() => new Promise<void>(resolve => { releaseFirst = resolve; }))
      .mockResolvedValueOnce(undefined);

    const enabling = setLogStreamActive(true);
    const disabling = setLogStreamActive(false);
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
    releaseFirst();
    await Promise.all([enabling, disabling]);

    expect(invoke.mock.calls).toEqual([
      ['set_log_stream_active', { active: true }],
      ['set_log_stream_active', { active: false }]
    ]);
  });

  it('serializes pause transitions so the backend cannot finish out of order', async () => {
    let releaseFirst!: () => void;
    invoke
      .mockImplementationOnce(() => new Promise<void>(resolve => { releaseFirst = resolve; }))
      .mockResolvedValueOnce(undefined);

    const pausing = setLogPaused(true);
    const resuming = setLogPaused(false);
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
    releaseFirst();
    await Promise.all([pausing, resuming]);

    expect(invoke.mock.calls).toEqual([
      ['toggle_log_pause', { pause: true }],
      ['toggle_log_pause', { pause: false }]
    ]);
  });
});
