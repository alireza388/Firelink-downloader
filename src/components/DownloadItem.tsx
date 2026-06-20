import React, { useEffect, useRef } from 'react';
import { useDownloadStore } from '../store/useDownloadStore';
import { useDownloadProgressStore } from '../store/downloadStore';
import { Play, Pause, MoreVertical, Clock, ArrowUp, ArrowDown } from 'lucide-react';
import type { DownloadItem as DownloadItemType } from '../bindings/DownloadItem';

interface DownloadItemProps {
  downloadId: string;
  index: number;
  tableGridTemplate: string;
  setContextMenu: (menu: { x: number; y: number; id: string }) => void;
  handlePause: (id: string) => void;
  handleResume: (item: DownloadItemType) => void;
  handleDoubleClick: (item: DownloadItemType) => void;
  getCategoryIcon: (category: string) => React.ReactNode;
}

export const DownloadItem = React.memo<DownloadItemProps>(({
  downloadId,
  index,
  tableGridTemplate,
  setContextMenu,
  handlePause,
  handleResume,
  handleDoubleClick,
  getCategoryIcon,
}) => {
  const download = useDownloadStore(state => state.downloads.find(d => d.id === downloadId));
  const pendingOrder = useDownloadStore(state => state.pendingOrder);
  const moveInQueue = useDownloadStore(state => state.moveInQueue);
  const queueIndex = pendingOrder.indexOf(downloadId);

  const progressBarRef = useRef<HTMLDivElement>(null);
  const statusTextRef = useRef<HTMLSpanElement>(null);
  const speedTextRef = useRef<HTMLSpanElement>(null);
  const etaTextRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!download || download.status !== 'downloading') return;

    const unsubscribe = useDownloadProgressStore.subscribe((state) => {
      const progress = state.progressMap[downloadId];
      if (!progress) return;

      if (progressBarRef.current) {
        progressBarRef.current.style.width = `${progress.fraction * 100}%`;
      }
      if (statusTextRef.current) {
        statusTextRef.current.innerText = `${(progress.fraction * 100).toFixed(0)}%`;
        statusTextRef.current.title = `${(progress.fraction * 100).toFixed(0)}%`;
      }
      if (speedTextRef.current) {
        speedTextRef.current.innerText = progress.speed;
        speedTextRef.current.title = progress.speed;
      }
      if (etaTextRef.current) {
        etaTextRef.current.innerText = progress.eta;
        etaTextRef.current.title = progress.eta;
      }
    });

    return () => unsubscribe();
  }, [downloadId, download?.status]);

  if (!download) return null;

  return (
    <div
      className={`download-row group cursor-default relative ${index % 2 !== 0 ? 'striped' : ''}`}
      style={{ gridTemplateColumns: tableGridTemplate }}
      onContextMenu={(e) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, id: download.id });
      }}
      onDoubleClick={() => handleDoubleClick(download)}
    >
      <div className="download-file-cell">
        <span className="shrink-0 text-text-muted">
          {getCategoryIcon(download.category)}
        </span>
        <span className="download-file-name" title={download.fileName}>
          {download.fileName}
        </span>
      </div>
      
      <div className="download-cell-truncate">
        <span className="tabular-nums" title={download.size && download.size !== '-' ? download.size : 'Unknown'}>
          {download.size && download.size !== '-' ? download.size : 'Unknown'}
        </span>
      </div>
      
      <div className="download-status-cell">
        {download.status === 'completed' ? (
          <span className="download-status download-status-completed" title="Completed">Completed</span>
        ) : (
          <>
            <div className="download-progress-track">
              <div
                ref={progressBarRef}
                className={`download-progress-fill ${
                  download.status === 'paused' ? 'paused' : 
                  download.status === 'processing' ? 'processing' :
                  download.status === 'queued' ? 'queued' :
                  download.status === 'retrying' ? 'retrying' : ''
                }`}
                style={{ width: `${(download.fraction || 0) * 100}%` }}
              />
            </div>
            <span 
            ref={statusTextRef}
            title={
              download.status === 'queued' && queueIndex !== -1
                ? `Queued #${queueIndex + 1}`
                : download.status === 'downloading'
                  ? `${((download.fraction || 0) * 100).toFixed(0)}%`
                  : download.status === 'processing'
                    ? 'Processing'
                    : download.status.charAt(0).toUpperCase() + download.status.slice(1)
            }
            className={`download-status flex items-center gap-1.5 ${
              download.status === 'paused' ? 'download-status-paused' : 
              download.status === 'failed' ? 'download-status-failed' :
                download.status === 'processing' ? 'download-status-processing' :
                download.status === 'downloading' ? 'download-status-downloading' : 
                download.status === 'queued' ? 'download-status-queued' :
                download.status === 'retrying' ? 'download-status-retrying' : ''
              }`}
            >
              {download.status === 'queued' && queueIndex !== -1 ? (
                <>
                  <Clock size={12} className="animate-pulse shrink-0" />
                  <span className="truncate">Queued #{queueIndex + 1}</span>
                </>
              ) : download.status === 'downloading' ? (
                `${((download.fraction || 0) * 100).toFixed(0)}%`
              ) : download.status === 'processing' ? (
                'Processing'
              ) : (
                download.status.charAt(0).toUpperCase() + download.status.slice(1)
              )}
            </span>
          </>
        )}
      </div>
      
      <div className="download-cell-truncate">
        <span
          ref={speedTextRef}
          className="tabular-nums"
          title={download.status === 'downloading' ? download.speed : download.status === 'processing' ? 'Processing…' : '-'}
        >
          {download.status === 'downloading' ? download.speed : download.status === 'processing' ? 'Processing…' : '-'}
        </span>
      </div>

      <div className="download-cell-truncate">
        <span
          ref={etaTextRef}
          className="tabular-nums"
          title={download.status === 'downloading' ? download.eta : download.status === 'processing' ? 'Muxing…' : '-'}
        >
          {download.status === 'downloading' ? download.eta : download.status === 'processing' ? 'Muxing…' : '-'}
        </span>
      </div>

      <div className="download-cell-right">
        <span
          className="truncate group-hover:hidden tabular-nums ml-auto"
          title={download.dateAdded ? new Date(download.dateAdded).toLocaleDateString() : '-'}
        >
          {download.dateAdded ? new Date(download.dateAdded).toLocaleDateString() : '-'}
        </span>
        
        <div
          className="hidden group-hover:flex items-center justify-end gap-0.5 w-full ml-auto"
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {download.status === 'queued' && queueIndex !== -1 && (
            <>
              <button 
                onClick={() => moveInQueue(download.id, 'up')}
                disabled={queueIndex === 0}
                className="app-icon-button h-7 w-7 disabled:opacity-40" 
                title="Move Up"
              >
                <ArrowUp size={14} />
              </button>
              <button 
                onClick={() => moveInQueue(download.id, 'down')}
                disabled={queueIndex === pendingOrder.length - 1}
                className="app-icon-button h-7 w-7 disabled:opacity-40" 
                title="Move Down"
              >
                <ArrowDown size={14} />
              </button>
            </>
          )}
          {(download.status === 'downloading' || download.status === 'processing' || download.status === 'retrying') && (
            <button onClick={() => handlePause(download.id)} className="app-icon-button h-7 w-7" title="Pause">
              <Pause size={14} fill="currentColor" />
            </button>
          )}
          {(download.status === 'ready' || download.status === 'paused') && (
            <button onClick={() => handleResume(download)} className="app-icon-button h-7 w-7" title={download.status === 'ready' ? 'Start' : 'Resume'}>
              <Play size={14} fill="currentColor" />
            </button>
          )}
          <button
            onClick={(e) => {
               e.stopPropagation();
               setContextMenu({ x: e.clientX, y: e.clientY, id: download.id });
            }}
            className="app-icon-button h-7 w-7"
            title="Options"
          >
            <MoreVertical size={14} />
          </button>
        </div>
      </div>
    </div>
  );
});
