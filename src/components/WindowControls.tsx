import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X } from 'lucide-react';

const appWindow = getCurrentWindow();

export function WindowControls() {
  return (
    <div className="window-controls" aria-label="Window controls">
      <button
        type="button"
        className="window-control"
        title="Minimize"
        aria-label="Minimize"
        onClick={() => void appWindow.minimize()}
      >
        <Minus size={13} strokeWidth={2.25} />
      </button>
      <button
        type="button"
        className="window-control"
        title="Maximize"
        aria-label="Maximize"
        onClick={() => void appWindow.toggleMaximize()}
      >
        <Square size={11} strokeWidth={2.25} />
      </button>
      <button
        type="button"
        className="window-control close"
        title="Close"
        aria-label="Close"
        onClick={() => void appWindow.close()}
      >
        <X size={14} strokeWidth={2.25} />
      </button>
    </div>
  );
}
