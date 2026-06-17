import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invokeCommand as invoke } from '../ipc';
import { save } from '@tauri-apps/plugin-dialog';
import { FileDown, Trash2, Terminal } from 'lucide-react';
import { WindowDragRegion } from './WindowDragRegion';

interface LogEntry {
  level: 'Trace' | 'Debug' | 'Info' | 'Warn' | 'Error';
  message: string;
}

export default function DiagnosticsView() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const MAX_LOG_LINES = 2000;

  useEffect(() => {
    const unlisten = listen<{ level: string; message: string }>('log', (event) => {
      const level = event.payload.level as LogEntry['level'];
      const message = event.payload.message;
      if (message.includes('[download]') && message.includes('%')) return;
      setLogs(prev => {
        const next = [...prev, { level, message }];
        return next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next;
      });
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const handleExport = async () => {
    try {
      const path = await save({
        defaultPath: 'Firelink-Diagnostics.log',
        filters: [{ name: 'Log Files', extensions: ['log'] }],
      });
      if (!path) return;
      await invoke('export_logs', { destPath: path });
    } catch (e) {
      console.error('Export failed:', e);
    }
  };

  const handleClear = () => setLogs([]);

  const severityClass = (level: string) => {
    switch (level) {
      case 'Error': return 'log-error';
      case 'Warn': return 'log-warn';
      case 'Info': return 'log-info';
      default: return 'log-debug';
    }
  };

  return (
    <div className="diagnostics-view flex-1 flex flex-col h-full overflow-hidden">
      <WindowDragRegion />

      {/* Toolbar */}
      <div className="diagnostics-toolbar flex items-center justify-between px-4 py-2 shrink-0">
        <div className="flex items-center gap-2 text-text-secondary">
          <Terminal size={16} strokeWidth={1.8} />
          <span className="text-[13px] font-semibold text-text-primary">Diagnostics Console</span>
          <span className="text-[11px] text-text-muted">({logs.length} entries)</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleClear}
            className="app-icon-button"
            title="Clear console"
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
      <div ref={scrollRef} className="diagnostics-console flex-1 overflow-y-auto p-3 font-mono text-[11px] leading-[1.5]">
        {logs.length === 0 && (
          <div className="text-text-muted italic select-none">Waiting for log entries...</div>
        )}
        {logs.map((entry, i) => (
          <div key={i} className={`log-line ${severityClass(entry.level)}`}>
            <span className="log-level-tag">[{entry.level}]</span>
            <span className="log-message">{entry.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
