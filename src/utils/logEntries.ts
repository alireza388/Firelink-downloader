export type LogLevel = 'Trace' | 'Debug' | 'Info' | 'Warn' | 'Error';

export interface LogEntry {
  level: LogLevel;
  message: string;
}

export const MAX_LOG_LINES = 2000;

const LEVEL_NAMES: LogLevel[] = ['Trace', 'Debug', 'Info', 'Warn', 'Error'];
const LIVE_LEVELS: Record<number, LogLevel> = {
  1: 'Trace',
  2: 'Debug',
  3: 'Info',
  4: 'Warn',
  5: 'Error'
};

const levelFromMessage = (message: string): LogLevel | undefined =>
  LEVEL_NAMES.find(level => message.includes(`[${level.toUpperCase()}]`));

export const persistedLogEntry = (message: string): LogEntry => ({
  level: levelFromMessage(message) || 'Debug',
  message
});

export const liveLogEntry = (
  numericLevel: number,
  message: string,
  now: Date = new Date()
): LogEntry => {
  const level = levelFromMessage(message) || LIVE_LEVELS[numericLevel] || 'Debug';
  const alreadyFormatted = /^\[\d{4}-\d{2}-\d{2}\]\[\d{2}:\d{2}:\d{2}\]\[(TRACE|DEBUG|INFO|WARN|ERROR)\]/.test(message);

  return {
    level,
    message: alreadyFormatted
      ? message
      : `[${now.toISOString().replace('T', ' ').substring(0, 19)}] [${level.toUpperCase()}] ${message}`
  };
};

export const appendBoundedLogEntries = (
  current: LogEntry[],
  additions: LogEntry[],
  limit = MAX_LOG_LINES
): LogEntry[] => {
  if (additions.length === 0) return current;
  const combined = [...current, ...additions];
  return combined.length > limit ? combined.slice(combined.length - limit) : combined;
};

export const pushBoundedLogEntry = (
  queue: LogEntry[],
  entry: LogEntry,
  limit = MAX_LOG_LINES
): void => {
  queue.push(entry);
  if (queue.length > limit) {
    queue.splice(0, queue.length - limit);
  }
};

// The live target writes after the file target, so entries received while the
// initial disk snapshot is loading can also appear at the snapshot's tail.
// Remove only the exact ordered overlap; global message de-duplication would
// incorrectly hide legitimate repeated log lines.
export const mergeLogSnapshotAndLiveEntries = (
  snapshot: LogEntry[],
  liveEntries: LogEntry[],
  limit = MAX_LOG_LINES
): LogEntry[] => {
  const maxOverlap = Math.min(snapshot.length, liveEntries.length);
  let overlap = 0;

  for (let candidate = maxOverlap; candidate > 0; candidate -= 1) {
    const snapshotStart = snapshot.length - candidate;
    let matches = true;
    for (let index = 0; index < candidate; index += 1) {
      if (snapshot[snapshotStart + index].message !== liveEntries[index].message) {
        matches = false;
        break;
      }
    }
    if (matches) {
      overlap = candidate;
      break;
    }
  }

  return appendBoundedLogEntries(snapshot, liveEntries.slice(overlap), limit);
};
