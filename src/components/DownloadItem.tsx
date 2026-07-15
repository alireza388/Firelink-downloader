import React from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useDownloadStore } from '../store/useDownloadStore';
import { useDownloadProgressStore } from '../store/downloadProgressStore';
import { Play, Pause, MoreVertical, Clock, ArrowUp, ArrowDown } from 'lucide-react';
import type { DownloadItem as DownloadItemType } from '../bindings/DownloadItem';
import { canPauseDownload, canStartDownload, startActionLabel } from '../utils/downloadActions';
import { isActiveDownloadStatus } from '../utils/downloads';
import {
  downloadProgressColorClass,
  formatDownloadTotal,
  resolveDownloadSizeDisplay
} from '../utils/downloadProgress';

interface DownloadItemProps {
  downloadId: string;
  index: number;
  tableGridTemplate: string;
  setContextMenu: (menu: { x: number; y: number; id: string }) => void;
  handlePause: (id: string, skipConfirm?: boolean) => void;
  handleResume: (item: DownloadItemType) => void;
  getCategoryIcon: (category: string) => React.ReactNode;
  selectedIds: Set<string>;
  onClick: (e: React.MouseEvent, item: DownloadItemType) => void;
}

export const DownloadItem = React.memo<DownloadItemProps>(({
  downloadId,
  index,
  tableGridTemplate,
  setContextMenu,
  handlePause,
  handleResume,
  getCategoryIcon,
  selectedIds,
  onClick,
}) => {
  const download = useDownloadStore(state => state.downloads.find(d => d.id === downloadId));
  const queueItems = useDownloadStore(useShallow(state => {
    const item = state.downloads.find(candidate => candidate.id === downloadId);
    if (!item) return [];
    const queueId = item.queueId;
    return state.downloads
      .filter(candidate =>
        candidate.queueId === queueId &&
        candidate.status !== 'completed' &&
        !(isActiveDownloadStatus(candidate.status) && candidate.status !== 'queued')
      )
      .sort((left, right) => (left.queuePosition ?? 0) - (right.queuePosition ?? 0))
      .map(candidate => candidate.id);
  }));
  const moveInQueue = useDownloadStore(state => state.moveInQueue);
  const liveProgress = useDownloadProgressStore(state => state.progressMap[downloadId]);
  const queueIndex = queueItems.indexOf(downloadId);

  if (!download) return null;

  const displayFraction = download.status === 'downloading'
    ? liveProgress?.fraction ?? download.fraction ?? 0
    : download.fraction ?? 0;
  const displayPercent = `${(displayFraction * 100).toFixed(0)}%`;
  const displaySpeed = download.status === 'downloading'
    ? liveProgress?.speed ?? download.speed
    : download.status === 'processing'
      ? 'Processing...'
      : '-';
  const displayEta = download.status === 'downloading'
    ? liveProgress?.eta ?? download.eta
    : download.status === 'processing'
      ? 'Muxing...'
      : '-';
  const sizeDisplay = resolveDownloadSizeDisplay({
    downloadedBytes: liveProgress?.downloaded_bytes ?? download.downloadedBytes,
    totalBytes: liveProgress?.total_bytes ?? download.totalBytes,
    totalIsEstimate: liveProgress?.total_is_estimate ?? download.totalIsEstimate,
    fallbackSize: download.size
  });
  const hasDownloadedAmount = download.status !== 'completed' &&
    Boolean(sizeDisplay.downloaded && sizeDisplay.total);
  const completedSizeLabel = download.status === 'completed'
    ? formatDownloadTotal(sizeDisplay)
    : sizeDisplay.fallback;

  return (
    <div
      className={`download-row group cursor-default relative ${index % 2 !== 0 ? 'striped' : ''} ${selectedIds.has(downloadId) ? 'is-selected' : ''}`}
      style={{ gridTemplateColumns: tableGridTemplate }}
      onClick={(e) => onClick(e, download)}
      onContextMenu={(e) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, id: download.id });
      }}
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
        <span
          className="tabular-nums"
          title={hasDownloadedAmount
            ? `${sizeDisplay.downloaded} downloaded of ${sizeDisplay.totalIsEstimate ? '~' : ''}${sizeDisplay.total} ${sizeDisplay.unit}`
            : completedSizeLabel}
          aria-label={hasDownloadedAmount
            ? `${sizeDisplay.downloaded} downloaded of ${sizeDisplay.totalIsEstimate ? 'approximately ' : ''}${sizeDisplay.total} ${sizeDisplay.unit}`
            : completedSizeLabel}
        >
          {hasDownloadedAmount ? (
            <>
              <span className={downloadProgressColorClass(download.status)}>{sizeDisplay.downloaded}</span>
              <span className="text-text-muted"> / </span>
              <span>
                {sizeDisplay.totalIsEstimate ? '~' : ''}{sizeDisplay.total} {sizeDisplay.unit}
              </span>
            </>
          ) : completedSizeLabel}
        </span>
      </div>
      
      <div className="download-status-cell">
        {download.status === 'completed' ? (
          <span className="download-status download-status-completed" title="Completed">Completed</span>
        ) : (
          <>
            <div className="download-progress-track">
              <div
                className={`download-progress-fill ${
                  download.status === 'paused' ? 'paused' : 
                  download.status === 'processing' ? 'processing' :
                  download.status === 'queued' || download.status === 'staged' ? 'queued' :
                  download.status === 'retrying' ? 'retrying' : ''
                }`}
                style={{ width: `${displayFraction * 100}%` }}
              />
            </div>
            <span 
            key={`status-${download.status}`}
            title={
              download.lastError && (download.status === 'failed' || download.status === 'retrying')
                ? download.lastError
                : (download.status === 'queued' || download.status === 'staged') && queueIndex !== -1
                ? `${download.status === 'staged' ? 'In queue' : 'Queued'} #${queueIndex + 1}`
                : download.status === 'downloading'
                  ? displayPercent
                  : download.status === 'processing'
                    ? 'Processing'
                    : download.status.charAt(0).toUpperCase() + download.status.slice(1)
            }
            className={`download-status flex items-center gap-1.5 ${
              download.status === 'paused' ? 'download-status-paused' : 
              download.status === 'failed' ? 'download-status-failed' :
                download.status === 'processing' ? 'download-status-processing' :
                download.status === 'downloading' ? 'download-status-downloading' : 
                download.status === 'queued' || download.status === 'staged' ? 'download-status-queued' :
                download.status === 'retrying' ? 'download-status-retrying' : ''
              }`}
            >
              {(download.status === 'queued' || download.status === 'staged') && queueIndex !== -1 ? (
                <>
                  <Clock size={12} className={download.status === 'queued' ? 'animate-pulse shrink-0' : 'shrink-0'} />
                  <span className="truncate">
                    {download.status === 'staged' ? 'In queue' : 'Queued'} #{queueIndex + 1}
                  </span>
                </>
              ) : download.status === 'downloading' ? (
                displayPercent
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
          key={`speed-${download.status}`}
          className="tabular-nums"
          title={displaySpeed}
        >
          {displaySpeed}
        </span>
      </div>

      <div className="download-cell-truncate">
        <span
          key={`eta-${download.status}`}
          className="tabular-nums"
          title={displayEta}
        >
          {displayEta}
        </span>
      </div>

      <div className="download-cell-right">
        <span
          className="truncate group-hover:hidden tabular-nums"
          title={download.dateAdded ? new Date(download.dateAdded).toLocaleDateString() : '-'}
        >
          {download.dateAdded ? new Date(download.dateAdded).toLocaleDateString() : '-'}
        </span>
        
        <div
          className="hidden group-hover:flex items-center justify-end gap-0.5 w-full ml-auto"
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {(download.status === 'queued' || download.status === 'staged') && queueIndex !== -1 && (
            <>
              <button 
                onClick={() => moveInQueue(selectedIds.has(download.id) ? Array.from(selectedIds) : download.id, 'up')}
                disabled={queueIndex === 0}
                className="app-icon-button h-7 w-7 disabled:opacity-40" 
                title="Move Up"
              >
                <ArrowUp size={14} />
              </button>
              <button 
                onClick={() => moveInQueue(selectedIds.has(download.id) ? Array.from(selectedIds) : download.id, 'down')}
                disabled={queueIndex === queueItems.length - 1}
                className="app-icon-button h-7 w-7 disabled:opacity-40" 
                title="Move Down"
              >
                <ArrowDown size={14} />
              </button>
            </>
          )}
          {canPauseDownload(download.status) && (
            <button onClick={() => handlePause(download.id)} className="app-icon-button h-7 w-7" title="Pause">
              <Pause size={14} fill="currentColor" />
            </button>
          )}
          {canStartDownload(download.status) && (
            <button onClick={() => handleResume(download)} className="app-icon-button h-7 w-7" title={startActionLabel(download.status)}>
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
