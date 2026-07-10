import { describe, expect, it } from 'vitest';
import {
  appendBoundedLogEntries,
  liveLogEntry,
  mergeLogSnapshotAndLiveEntries,
  persistedLogEntry,
  pushBoundedLogEntry,
  type LogEntry
} from './logEntries';

const entry = (message: string): LogEntry => ({ level: 'Info', message });

describe('log entry streaming', () => {
  it('derives levels from persisted formatted lines', () => {
    expect(persistedLogEntry('[2026-07-10][18:00:00][ERROR][firelink] failed')).toEqual({
      level: 'Error',
      message: '[2026-07-10][18:00:00][ERROR][firelink] failed'
    });
  });

  it('does not double-format backend Webview log lines', () => {
    const message = '[2026-07-10][18:00:00][WARN][firelink] retrying';
    expect(liveLogEntry(4, message).message).toBe(message);
  });

  it('formats an unformatted plugin event with its numeric level', () => {
    expect(liveLogEntry(3, 'download started', new Date('2026-07-10T14:30:00Z'))).toEqual({
      level: 'Info',
      message: '[2026-07-10 14:30:00] [INFO] download started'
    });
  });

  it('merges only the ordered snapshot-to-stream overlap', () => {
    expect(mergeLogSnapshotAndLiveEntries(
      [entry('one'), entry('repeat'), entry('three')],
      [entry('three'), entry('repeat'), entry('four')]
    ).map(item => item.message)).toEqual(['one', 'repeat', 'three', 'repeat', 'four']);
  });

  it('bounds burst updates to the newest entries', () => {
    expect(appendBoundedLogEntries(
      [entry('old')],
      Array.from({ length: 5 }, (_, index) => entry(`new-${index}`)),
      3
    ).map(item => item.message)).toEqual(['new-2', 'new-3', 'new-4']);
  });

  it('bounds the mutable pre-render queue without copying on every event', () => {
    const queue = [entry('old')];
    for (let index = 0; index < 5; index += 1) {
      pushBoundedLogEntry(queue, entry(`new-${index}`), 3);
    }
    expect(queue.map(item => item.message)).toEqual(['new-2', 'new-3', 'new-4']);
  });
});
