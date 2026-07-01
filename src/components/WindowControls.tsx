import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, Square, X } from 'lucide-react';
import type { PointerEvent } from 'react';

const appWindow = getCurrentWindow();

const stopTitlebarDrag = (event: PointerEvent<HTMLButtonElement>) => {
  event.stopPropagation();
};

export function WindowControls() {
  return (
    <div className="window-controls" aria-label="Window controls">
      <button
        type="button"
        className="window-control close"
        title="Close"
        aria-label="Close"
        onPointerDown={stopTitlebarDrag}
        onClick={(event) => {
          event.stopPropagation();
          void appWindow.close();
        }}
      >
        <X size={10} strokeWidth={3} />
      </button>
      <button
        type="button"
        className="window-control minimize"
        title="Minimize"
        aria-label="Minimize"
        onPointerDown={stopTitlebarDrag}
        onClick={(event) => {
          event.stopPropagation();
          void appWindow.minimize();
        }}
      >
        <Minus size={10} strokeWidth={3} />
      </button>
      <button
        type="button"
        className="window-control maximize"
        title="Maximize"
        aria-label="Maximize"
        onPointerDown={stopTitlebarDrag}
        onClick={(event) => {
          event.stopPropagation();
          void appWindow.toggleMaximize();
        }}
      >
        <Square size={8} strokeWidth={3} />
      </button>
    </div>
  );
}
