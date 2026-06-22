import { useEffect, useRef, useState } from 'react';
import { invokeCommand as invoke } from '../ipc';
import { save } from '@tauri-apps/plugin-dialog';
import { attachLogger } from '@tauri-apps/plugin-log';
import { FileDown, Trash2, Terminal, Filter, Play, Pause, Info, Copy } from 'lucide-react';
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
  const [isPaused, setIsPaused] = useState(false);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; text: string } | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const rawLineCountRef = useRef(0);

  const MAX_LOG_LINES = 2000;

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    const init = async () => {
      try {
        const [lines, currentPauseState] = await Promise.all([
          invoke('read_logs', { limit: MAX_LOG_LINES }),
          invoke('is_log_paused')
        ]);
        if (!active) return;
        setIsPaused(currentPauseState);
        if (!active) return;
        const initialLogs = lines.map(message => {
          const level: LogEntry['level'] = message.includes('[ERROR]') ? 'Error'
            : message.includes('[WARN]') ? 'Warn'
              : message.includes('[INFO]') ? 'Info'
                : message.includes('[TRACE]') ? 'Trace'
                  : 'Debug';
          return { level, message };
        });
        
        setLogs(initialLogs);
        rawLineCountRef.current = initialLogs.length;

        unlisten = await attachLogger((log) => {
          if (!active) return;
          const levelStr: LogEntry['level'] = log.level === 5 ? 'Error'
            : log.level === 4 ? 'Warn'
              : log.level === 3 ? 'Info'
                : log.level === 1 ? 'Trace'
                  : 'Debug';
          
          const timeStr = new Date().toISOString().replace('T', ' ').substring(0, 19);
          const formattedMsg = `[${timeStr}] [${levelStr.toUpperCase()}] ${log.message}`;

          setLogs(prev => {
            const newLogs = [...prev, { level: levelStr, message: formattedMsg }];
            rawLineCountRef.current = newLogs.length;
            if (newLogs.length > MAX_LOG_LINES + 500) {
              const trimmed = newLogs.slice(newLogs.length - MAX_LOG_LINES);

              return trimmed;
            }
            return newLogs;
          });
        });
      } catch (e) {
        console.error('Failed to init logs:', e);
      }
    };
    void init();
    
    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  useEffect(() => {
    const handleCloseMenu = () => setContextMenu(null);
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape') setContextMenu(null); };
    window.addEventListener('click', handleCloseMenu);
    window.addEventListener('keydown', handleEscape);
    return () => {
      window.removeEventListener('click', handleCloseMenu);
      window.removeEventListener('keydown', handleEscape);
    };
  }, []);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    const selection = window.getSelection()?.toString();
    if (selection && selection.trim().length > 0) {
      setContextMenu({ x: e.clientX, y: e.clientY, text: selection });
    } else {
      setContextMenu(null);
    }
  };

  const handleCopy = async () => {
    if (contextMenu?.text) {
      try {
        await navigator.clipboard.writeText(contextMenu.text);
        addToast({ message: 'Copied to clipboard', variant: 'success' });
      } catch (err) {
        console.error('Clipboard write error:', err);
        addToast({ message: 'Failed to copy to clipboard', variant: 'error' });
      }
    }
    setContextMenu(null);
  };

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

  const handleClear = async () => {
    setLogs([]);
    await invoke('clear_logs').catch(console.error);
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
            onClick={async () => {
              const newState = !isPaused;
              setIsPaused(newState);
              await invoke('toggle_log_pause', { pause: newState }).catch(console.error);
            }}
            className={`app-icon-button ${isPaused ? 'text-accent' : ''}`}
            title={isPaused ? "Resume logs" : "Pause logs system"}
          >
            {isPaused ? <Play size={14} /> : <Pause size={14} />}
          </button>
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

      {/* Privacy Hint */}
      <div className="bg-black/10 border-y border-border-modal px-4 py-2 shrink-0 flex items-center gap-2 text-text-muted text-[10px] select-none">
        <Info size={12} className="text-text-muted opacity-80 shrink-0" />
        <span className="opacity-90 leading-tight">
          <strong className="font-medium text-text-primary mr-1">Privacy Note:</strong>
          Telemetry securely captures basic hardware capabilities (OS, CPU, RAM) exclusively for troubleshooting. No unique identifiers or sensitive paths are collected. Logs remain entirely offline on your device until manually exported.
        </span>
      </div>

      {/* Console */}
      <div
        ref={scrollRef}
        onContextMenu={handleContextMenu}
        className="logs-console flex-1 overflow-y-auto p-3 font-mono text-[11px] leading-[1.5] select-text"
        style={{ userSelect: 'text', WebkitUserSelect: 'text' }}
      >
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

      {/* Context Menu */}
      {contextMenu && (
        <div
          role="menu"
          className="app-modal fixed z-50 min-w-[150px] overflow-visible py-1.5 text-[12px] font-medium text-text-primary"
          style={{ left: Math.min(contextMenu.x, window.innerWidth - 150), top: Math.min(contextMenu.y, window.innerHeight - 50) }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="w-full text-left px-3 py-2 flex items-center hover:bg-item-hover transition-colors"
            onClick={handleCopy}
          >
            <Copy size={13} className="mr-2 text-text-secondary" />
            Copy
          </button>
        </div>
      )}
    </div>
  );
}
