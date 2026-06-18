import React, { useState, useEffect } from 'react';
import { useDownloadStore, DownloadItem } from '../store/useDownloadStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { SidebarFilter } from './Sidebar';
import { Play, Pause, Plus, FileText, Image as ImageIcon, Music, Film, Box, Archive, FileQuestion, PanelLeft, ArrowDownCircle, Command } from 'lucide-react';
import { DownloadItem as DownloadItemComponent } from './DownloadItem';
import { invokeCommand as invoke } from '../ipc';
import { homeDir } from '@tauri-apps/api/path';

interface DownloadTableProps {
  filter: SidebarFilter;
}

export const DownloadTable: React.FC<DownloadTableProps> = ({ filter }) => {
  const { downloads, toggleAddModal, openDeleteModal, redownload } = useDownloadStore();
  const { isSidebarVisible, toggleSidebar } = useSettingsStore();

  const isMac = navigator.userAgent.includes('Mac');

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [columnWidths, setColumnWidths] = useState([340, 100, 220, 100, 80, 170]);
  const columnMinimums = [0, 58, 92, 58, 48, 112];
  const tableGridTemplate = columnWidths.map((width, index) => `minmax(${columnMinimums[index]}px, ${width}fr)`).join(' ');

  const startColumnResize = (index: number, event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = columnWidths[index];

    const handlePointerMove = (moveEvent: PointerEvent) => {
      const nextWidth = Math.max(columnMinimums[index], startWidth + moveEvent.clientX - startX);
      setColumnWidths(widths => widths.map((width, columnIndex) => columnIndex === index ? nextWidth : width));
    };

    const handlePointerUp = () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handlePointerUp);
      document.body.classList.remove('is-resizing');
    };

    document.body.classList.add('is-resizing');
    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp);
  };

  useEffect(() => {
    const handleCloseMenu = () => setContextMenu(null);
    window.addEventListener('click', handleCloseMenu);
    return () => window.removeEventListener('click', handleCloseMenu);
  }, []);

  const resolvePath = async (dir: string, file: string) => {
    let resolvedDir = dir;
    if (dir.startsWith('~/')) {
      const home = await homeDir();
      resolvedDir = home + '/' + dir.slice(2);
    } else if (dir === '~') {
      resolvedDir = await homeDir();
    }
    const separator = resolvedDir.endsWith('/') ? '' : '/';
    return resolvedDir + separator + file;
  };

  const filteredDownloads = downloads.filter((d: DownloadItem) => {
    if (filter.startsWith('queue:')) {
      return d.queueId === filter.replace('queue:', '');
    }
    switch (filter) {
      case 'all': return true;
      case 'active': return d.status === 'downloading';
      case 'completed': return d.status === 'completed';
      case 'unfinished': return d.status !== 'completed';
      default: return d.category === filter;
    }
  });

  const getFilterTitle = () => {
    if (filter.startsWith('queue:')) {
      const qid = filter.replace('queue:', '');
      const queue = useDownloadStore.getState().queues.find(q => q.id === qid);
      return queue ? queue.name : 'Unknown Queue';
    }
    switch (filter) {
      case 'all': return 'All Downloads';
      case 'active': return 'Active';
      case 'completed': return 'Completed';
      case 'unfinished': return 'Unfinished';
      default: return filter;
    }
  };

  const handlePause = async (id: string) => {
    try {
      await invoke('pause_download', { id });
    } catch (e) {
      console.error("Failed to pause:", e);
    }
  };

  const handleResume = (item: DownloadItem) => {
    useDownloadStore.getState().resumeDownload(item.id);
  };

  const handleDelete = (id: string) => {
    openDeleteModal(id);
  };

  const contextItem = contextMenu ? downloads.find(d => d.id === contextMenu.id) : null;


  const getCategoryIcon = (category: string) => {
    switch(category) {
      case 'Musics': return <Music size={16} className="text-pink-400" />;
      case 'Movies': return <Film size={16} className="text-red-400" />;
      case 'Documents': return <FileText size={16} className="text-blue-400" />;
      case 'Applications': return <Box size={16} className="text-indigo-400" />;
      case 'Pictures': return <ImageIcon size={16} className="text-purple-400" />;
      case 'Compressed': return <Archive size={16} className="text-amber-600" />;
      case 'Other': return <FileQuestion size={16} className="text-gray-400" />;
      default: return <FileQuestion size={16} className="text-gray-400" />;
    }
  }

  return (
    <div className="downloads-view flex-1 flex flex-col h-full min-w-0">
      <div className={`main-titlebar ${!isSidebarVisible ? 'pl-[80px]' : ''}`} data-tauri-drag-region>
        {!isSidebarVisible && (
          <button
            onClick={toggleSidebar}
            className="app-icon-button relative z-50 h-7 w-7 mr-2"
            title="Show Sidebar"
          >
            <PanelLeft size={16} strokeWidth={2} />
          </button>
        )}
        <div className="main-titlebar-title cursor-default" data-tauri-drag-region>Firelink</div>

        <div className="main-control-group">
          <button className="main-control-button primary" onClick={() => toggleAddModal(true)} title="Add Download">
            <Plus size={16} />
          </button>

          <button 
            className="main-control-button" 
            disabled={filteredDownloads.length === 0}
            onClick={() => {
              filteredDownloads.filter(d => d.status === 'paused').forEach(d => handleResume(d));
            }}
            title="Resume All"
          >
            <Play size={15} fill="currentColor" />
          </button>

          <button 
            className="main-control-button" 
            disabled={filteredDownloads.length === 0}
            onClick={() => {
              filteredDownloads.filter(d => d.status === 'downloading').forEach(d => handlePause(d.id));
            }}
            title="Pause All"
          >
            <Pause size={15} fill="currentColor" />
          </button>
        </div>
      </div>

      <div className="downloads-content-header">
        <div className="downloads-title">
          {getFilterTitle()}
          <span className="downloads-count">{filteredDownloads.length}</span>
        </div>
      </div>

      <div className="downloads-table flex-1 flex flex-col">
        {filteredDownloads.length === 0 ? (
          <div className="downloads-empty-state">
            <ArrowDownCircle aria-hidden="true" />
            <div className="downloads-empty-title">No Downloads</div>
            <div className="downloads-empty-description flex items-center justify-center mt-2.5 text-[13px] text-text-muted">
              Click <Plus size={15} className="text-accent stroke-[3] mx-1.5" /> button or 
              <span className="flex items-center mx-1.5">
                <span className="flex items-center justify-center px-1.5 py-0.5 bg-item-hover rounded border border-border-color shadow-sm min-w-[22px] min-h-[22px]">
                  {isMac ? <Command size={12} strokeWidth={2.5} className="text-text-primary" /> : <span className="text-[10px] font-bold text-text-primary">Ctrl</span>}
                </span>
                <span className="text-accent font-bold mx-1.5 text-[14px]">+</span>
                <span className="flex items-center justify-center px-1.5 py-0.5 bg-item-hover rounded border border-border-color shadow-sm min-w-[22px] min-h-[22px]">
                  <span className="text-[11px] font-bold text-text-primary">V</span>
                </span>
              </span>
              to add downloads
            </div>
          </div>
        ) : (
          <>
            <div className="download-table-scroll">
            <div className="download-table-header" style={{ gridTemplateColumns: tableGridTemplate }}>
              {['File Name', 'Size', 'Status', 'Speed', 'ETA', 'Date Added'].map((label, index) => (
                <div key={label} className={index === 5 ? 'download-cell-right' : undefined}>
                  <span>{label}</span>
                  <div
                    className="column-resize-handle"
                    onPointerDown={(event) => startColumnResize(index, event)}
                  />
                </div>
              ))}
            </div>

            <div className="download-table-body">
            <div className="h-full overflow-auto flex flex-col">
              {filteredDownloads.map((d, index) => (
                <DownloadItemComponent
                  key={d.id}
                  downloadId={d.id}
                  index={index}
                  tableGridTemplate={tableGridTemplate}
                  setContextMenu={setContextMenu}
                  handlePause={handlePause}
                  handleResume={handleResume}
                  getCategoryIcon={getCategoryIcon}
                />
              ))}
              {Array.from({ length: Math.max(0, 50 - filteredDownloads.length) }).map((_, i) => {
                const globalIndex = filteredDownloads.length + i;
                return (
                  <div
                    key={`ghost-${i}`}
                    className={`download-ghost-row ${globalIndex % 2 !== 0 ? 'striped' : ''}`}
                  />
                );
              })}
              <div className="flex-1 bg-transparent pointer-events-none"></div>
            </div>
            </div>
            </div>
          </>
        )}
      </div>

      {/* Floating Context Menu */}
      {contextMenu && contextItem && (
        <div
          className="app-modal fixed z-50 min-w-[180px] overflow-hidden py-1.5 text-[12px] font-medium text-text-primary"
          style={{
             top: Math.min(contextMenu.y, window.innerHeight - 300),
             left: Math.min(contextMenu.x, window.innerWidth - 200)
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {contextItem.status === 'completed' && (
            <button
              onClick={async () => {
                setContextMenu(null);
                try {
                  const fullPath = await resolvePath(contextItem.destination || '~/Downloads', contextItem.fileName);
                  await invoke('open_downloaded_file', { path: fullPath });
                } catch (e) {
                  console.error("Failed to open file:", e);
                }
              }}
              className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
            >
              Open File
            </button>
          )}

          <button
            onClick={async () => {
              setContextMenu(null);
              try {
                const fullPath = await resolvePath(contextItem.destination || '~/Downloads', contextItem.fileName);
                await invoke('reveal_in_file_manager', { path: fullPath });
              } catch (e) {
                console.error("Failed to show in folder:", e);
              }
            }}
            className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
          >
            Show in Finder
          </button>

          <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

          {(contextItem.status === 'downloading' || contextItem.status === 'queued' || contextItem.status === 'retrying') && (
            <button
              onClick={() => {
                setContextMenu(null);
                handlePause(contextItem.id);
              }}
              className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
            >
              Pause
            </button>
          )}

          {(contextItem.status === 'paused' || contextItem.status === 'failed' || contextItem.status === 'retrying') && (
            <button
              onClick={() => {
                setContextMenu(null);
                handleResume(contextItem);
              }}
              className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
            >
              Resume
            </button>
          )}

          {['completed', 'failed', 'paused', 'retrying'].includes(contextItem.status) && (
            <button
              onClick={() => {
                setContextMenu(null);
                redownload(contextItem.id);
              }}
              className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
            >
              Redownload
            </button>
          )}

          <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

          <button
            onClick={() => {
              setContextMenu(null);
              navigator.clipboard.writeText(contextItem.url);
            }}
            className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
          >
            Copy Address
          </button>

          {contextItem.status === 'completed' && (
            <button
              onClick={async () => {
                setContextMenu(null);
                const fullPath = await resolvePath(contextItem.destination || '~/Downloads', contextItem.fileName);
                navigator.clipboard.writeText(fullPath);
              }}
              className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
            >
              Copy File Path
            </button>
          )}

          <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

          <button
            onClick={() => {
              setContextMenu(null);
              handleDelete(contextItem.id);
            }}
            className="w-full text-left px-3 py-2 text-red-400 hover:bg-red-500/10 transition-colors"
          >
            Remove
          </button>

          <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

          <button
            onClick={() => {
              setContextMenu(null);
              useDownloadStore.getState().setSelectedPropertiesDownloadId(contextItem.id);
            }}
            className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
          >
            Properties
          </button>
        </div>
      )}

    </div>
  );
};
