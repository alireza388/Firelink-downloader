import { getCurrentWindow } from '@tauri-apps/api/window';

interface WindowDragRegionProps {
  className?: string;
}

export function WindowDragRegion({ className = '' }: WindowDragRegionProps) {
  return (
    <div
      className={`h-10 shrink-0 cursor-default ${className}`}
      data-tauri-drag-region
      onPointerDown={(event) => {
        if (event.button === 0) {
          void getCurrentWindow().startDragging();
        }
      }}
    />
  );
}
