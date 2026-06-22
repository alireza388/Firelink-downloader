import { useEffect, useRef, useState } from 'react';
import { invokeCommand as invoke } from '../ipc';
import { save } from '@tauri-apps/plugin-dialog';
import { FileDown, Trash2, Terminal, Filter } from 'lucide-react';
import { WindowDragRegion } from './WindowDragRegion';
import { useToast } from '../contexts/ToastContext';

interface LogEntry {
  level: 'Trace' | 'Debug' | 'Info' | 'Warn' | 'Error';
  message: string;
}

export default function LogsView() {
  const { addToast } = useToast();
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [levelFilter, setLevelFilter] = useState<LogEntry['level'] | 'All'>('All');
  const scrollRef = useRef<HTMLDivElement>(null);
  const rawLineCountRef = useRef(0);
  const clearedThroughRef = useRef(0);
  const lastSnapshotRef = useRef('');
  const MAX_LOG_LINES = 2000;

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        const lines = await invoke('read_logs', { limit: MAX_LOG_LINES });
        if (!active) return;
        if (lines.length < clearedThroughRef.current) {
          clearedThroughRef.current = 0;
        }
        const snapshot = `${lines.length}:${lines[lines.length - 1] || ''}`;
        if (snapshot === lastSnapshotRef.current) return;
        lastSnapshotRef.current = snapshot;
        rawLineCountRef.current = lines.length;
        setLogs(lines.slice(clearedThroughRef.current).map(message => {
          const level = message.includes('[ERROR]') ? 'Error'
            : message.includes('[WARN]') ? 'Warn'
              : message.includes('[INFO]') ? 'Info'
                : message.includes('[TRACE]') ? 'Trace'
                  : 'Debug';
          return { level, message };
        }));
      } catch {
        if (active) setLogs([]);
      }
    };
    void refresh();
    const interval = window.setInterval(refresh, 2000);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const handleExport = async () => {
    try {
      const path = await save({
        defaultPath: 'Firelink-Support-Logs.log',
        filters: [{ name: 'Log Files', extensions: ['log'] }],
      });
      if (!path) return;
      await invoke('export_logs', { destPath: path });
      addToast({ message: 'Support logs exported', variant: 'success' });
    } catch (e) {
      console.error('Export failed:', e);
      addToast({ message: `Could not export logs: ${String(e)}`, variant: 'error', isActionable: true });
    }
  };

  const handleClear = () => {
    clearedThroughRef.current = rawLineCountRef.current;
    setLogs([]);
  };

  const severityClass = (level: string) => {
    switch (level) {
      case 'Error': return 'log-error';
      case 'Warn': return 'log-warn';
      case 'Info': return 'log-info';
      default: return 'log-debug';
    }
  };

  return (
    <div className="logs-view flex-1 flex flex-col h-full overflow-hidden">
      <WindowDragRegion />

      {/* Toolbar */}
      <div className="logs-toolbar flex items-center justify-between px-4 py-2 shrink-0">
        <div className="flex items-center gap-2 text-text-secondary">
          <Terminal size={16} strokeWidth={1.8} />
          <span className="text-[13px] font-semibold text-text-primary">Logs</span>
          <span className="text-[11px] text-text-muted">({logs.length} entries)</span>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5">
            <Filter size={13} className="text-text-muted" />
            <select
              value={levelFilter}
              onChange={e => setLevelFilter(e.target.value as LogEntry['level'] | 'All')}
              className="bg-bg-input border border-border-modal rounded px-1.5 py-0.5 text-[11px] text-text-primary focus:outline-none focus:border-accent"
            >
              <option value="All">All Levels</option>
              <option value="Error">Error</option>
              <option value="Warn">Warn</option>
              <option value="Info">Info</option>
              <option value="Debug">Debug</option>
              <option value="Trace">Trace</option>
            </select>
          </div>
          <div className="w-[1px] h-4 bg-border-modal mx-0.5" />
          <button
            onClick={handleClear}
            className="app-icon-button"
            title="Clear displayed logs"
          >
            <Trash2 size={14} />
          </button>
          <button
            onClick={handleExport}
            className="app-button px-3 text-[11px] gap-1.5"
            title="Export logs"
          >
            <FileDown size={13} />
            Export Logs
          </button>
        </div>
      </div>

      {/* Console */}
      <div ref={scrollRef} className="logs-console flex-1 overflow-y-auto p-3 font-mono text-[11px] leading-[1.5]">
        {logs.length === 0 && (
          <div className="text-text-muted italic select-none">No persisted log entries are available yet.</div>
        )}
        {logs.filter(entry => levelFilter === 'All' || entry.level === levelFilter).map((entry, i) => (
          <div key={i} className={`log-line ${severityClass(entry.level)}`}>
            <span className="log-level-tag">[{entry.level}]</span>
            <span className="log-message">{entry.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
