import React, { useState, useEffect } from 'react';
import { useDownloadStore, DownloadItem } from '../store/useDownloadStore';
import { useToast } from '../contexts/ToastContext';
import { useSettingsStore } from '../store/useSettingsStore';
import { SidebarFilter } from './Sidebar';
import { Play, Pause, Plus, FileText, Image as ImageIcon, Music, Film, Box, Archive, FileQuestion, PanelLeft, ArrowDownCircle, Command, ChevronRight } from 'lucide-react';
import { DownloadItem as DownloadItemComponent } from './DownloadItem';
import { invokeCommand as invoke } from '../ipc';
import {
  resolveCategoryDestination,
  resolveDownloadFilePath
} from '../utils/downloadLocations';
import {
  canPauseDownload,
  canRedownload,
  canStartDownload,
  startActionLabel
} from '../utils/downloadActions';

interface DownloadTableProps {
  filter: SidebarFilter;
}

const DEFAULT_COLUMN_WIDTHS = [340, 100, 220, 100, 80, 170];
const COLUMN_WIDTHS_STORAGE_KEY = 'firelink-download-column-widths';

export const DownloadTable: React.FC<DownloadTableProps> = ({ filter }) => {
  const { downloads, queues, assignToQueue, toggleAddModal, openDeleteModal, redownload } = useDownloadStore();
  const { isSidebarVisible, toggleSidebar } = useSettingsStore();
  const { addToast } = useToast();

  const isMac = navigator.userAgent.includes('Mac');

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [lastSelectedId, setLastSelectedId] = useState<string | null>(null);
  const [columnWidths, setColumnWidths] = useState(() => {
    try {
      const stored = JSON.parse(window.localStorage.getItem(COLUMN_WIDTHS_STORAGE_KEY) || 'null');
      return Array.isArray(stored) &&
        stored.length === DEFAULT_COLUMN_WIDTHS.length &&
        stored.every(value => typeof value === 'number' && Number.isFinite(value))
        ? stored
        : DEFAULT_COLUMN_WIDTHS;
    } catch {
      return DEFAULT_COLUMN_WIDTHS;
    }
  });
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
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setContextMenu(null);
    };
    window.addEventListener('click', handleCloseMenu);
    window.addEventListener('keydown', handleEscape);
    return () => {
      window.removeEventListener('click', handleCloseMenu);
      window.removeEventListener('keydown', handleEscape);
    };
  }, []);

  useEffect(() => {
    window.localStorage.setItem(COLUMN_WIDTHS_STORAGE_KEY, JSON.stringify(columnWidths));
  }, [columnWidths]);


  const showInteractionError = (message: string, error: unknown) => {
    const detail = error instanceof Error ? error.message : String(error);
    addToast({ message: `${message}: ${detail}`, variant: 'error', isActionable: true });
  };

  const getDownloadPath = async (item: DownloadItem) => {
    const fileName = item.fileName?.trim();
    if (!fileName) return null;
    const settings = useSettingsStore.getState();
    const destination = item.destination ||
      await resolveCategoryDestination(settings, item.category);
    return resolveDownloadFilePath(destination, fileName);
  };

  const openProperties = (id: string) => {
    useDownloadStore.getState().setSelectedPropertiesDownloadId(id);
  };

  const openDownloadFile = async (item: DownloadItem) => {
    if (item.status !== 'completed') {
      openProperties(item.id);
      return;
    }

    const fullPath = await getDownloadPath(item);
    if (!fullPath) {
      openProperties(item.id);
      return;
    }

    try {
      await invoke('open_downloaded_file', { path: fullPath });
    } catch (error) {
      console.error("Failed to open file:", error);
      showInteractionError('Could not open downloaded file', error);
    }
  };

  const revealDownloadFile = async (item: DownloadItem) => {
    const pathToReveal = await getDownloadPath(item);

    if (!pathToReveal) {
      openProperties(item.id);
      return;
    }

    try {
      await invoke('reveal_in_file_manager', { path: pathToReveal });
    } catch (error) {
      console.error("Failed to show in Finder:", error);
      showInteractionError('Could not show download in Finder', error);
    }
  };

  const handleDownloadDoubleClick = (item: DownloadItem) => {
    if (item.status === 'completed') {
      void openDownloadFile(item);
      return;
    }

    openProperties(item.id);
  };

  const filteredDownloads = downloads.filter((d: DownloadItem) => {
    if (filter.startsWith('queue:')) {
      return d.queueId === filter.replace('queue:', '') && d.status !== 'completed';
    }
    switch (filter) {
      case 'all': return true;
      case 'active': return d.status === 'downloading';
      case 'completed': return d.status === 'completed';
      case 'unfinished': return d.status !== 'completed';
      default: return d.category === filter;
    }
  });
  const handleItemClick = (e: React.MouseEvent, item: DownloadItem) => {
    if (e.metaKey || e.ctrlKey) {
      const newSelected = new Set(selectedIds);
      if (newSelected.has(item.id)) {
        newSelected.delete(item.id);
      } else {
        newSelected.add(item.id);
      }
      setSelectedIds(newSelected);
      setLastSelectedId(item.id);
    } else if (e.shiftKey && lastSelectedId) {
      const currentIndex = filteredDownloads.findIndex(d => d.id === item.id);
      const lastIndex = filteredDownloads.findIndex(d => d.id === lastSelectedId);
      
      if (currentIndex !== -1 && lastIndex !== -1) {
        const start = Math.min(currentIndex, lastIndex);
        const end = Math.max(currentIndex, lastIndex);
        
        const newSelected = new Set(selectedIds);
        for (let i = start; i <= end; i++) {
          newSelected.add(filteredDownloads[i].id);
        }
        setSelectedIds(newSelected);
      }
    } else {
      setSelectedIds(new Set([item.id]));
      setLastSelectedId(item.id);
    }
  };

  const handleContextMenu = (menu: { x: number; y: number; id: string }) => {
    if (!selectedIds.has(menu.id)) {
      setSelectedIds(new Set([menu.id]));
      setLastSelectedId(menu.id);
    }
    setContextMenu(menu);
  };


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
      showInteractionError('Could not pause download', e);
    }
  };

  const handleResume = (item: DownloadItem) => {
    useDownloadStore.getState().resumeDownload(item.id);
  };

  const handleDelete = (ids: string | string[]) => {
    openDeleteModal(ids);
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
              filteredDownloads
                .filter(d => canStartDownload(d.status))
                .forEach(d => handleResume(d));
            }}
            title="Resume All"
          >
            <Play size={15} fill="currentColor" />
          </button>

          <button 
            className="main-control-button" 
            disabled={filteredDownloads.length === 0}
            onClick={() => {
              filteredDownloads.filter(d => canPauseDownload(d.status)).forEach(d => handlePause(d.id));
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
                  setContextMenu={handleContextMenu}
                  handlePause={handlePause}
                  handleResume={handleResume}
                  handleDoubleClick={handleDownloadDoubleClick}
                  getCategoryIcon={getCategoryIcon}
                  isSelected={selectedIds.has(d.id)}
                  onClick={handleItemClick}
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
          role="menu"
          className="app-modal fixed z-50 min-w-[180px] overflow-visible py-1.5 text-[12px] font-medium text-text-primary"
          style={{
             top: Math.min(contextMenu.y, window.innerHeight - 300),
             left: Math.min(contextMenu.x, window.innerWidth - 200)
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {selectedIds.size > 1 ? (
            <>
              {/* Multi-Select Context Menu */}
              <button
                onClick={() => {
                  setContextMenu(null);
                  Array.from(selectedIds).forEach(id => {
                    const item = downloads.find(d => d.id === id);
                    if (item && canStartDownload(item.status)) {
                      handleResume(item);
                    }
                  });
                }}
                className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
              >
                Start/Resume
              </button>

              <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

              <div className="group relative">
                <button className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors flex justify-between items-center">
                  Add to Queue
                  <ChevronRight size={14} />
                </button>
                <div className="absolute left-full top-0 hidden group-hover:block ml-1 min-w-[150px] bg-bg-modal border border-border-modal rounded-lg shadow-lg py-1.5 z-50">
                  {queues.map(q => (
                    <button key={q.id} onClick={() => {
                      setContextMenu(null);
                      assignToQueue(Array.from(selectedIds), q.id);
                    }} className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors text-[12px]">
                      {q.name}
                    </button>
                  ))}
                </div>
              </div>

              <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

              <button
                onClick={() => {
                  setContextMenu(null);
                  const urls = Array.from(selectedIds)
                    .map(id => downloads.find(d => d.id === id)?.url)
                    .filter(Boolean)
                    .join('\n');
                  navigator.clipboard.writeText(urls).catch(error => {
                    showInteractionError('Could not copy addresses', error);
                  });
                }}
                className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
              >
                Copy Address
              </button>

              <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

              <button
                onClick={() => {
                  setContextMenu(null);
                  handleDelete(Array.from(selectedIds));
                }}
                className="w-full text-left px-3 py-2 text-red-400 hover:bg-red-500/10 transition-colors"
              >
                Remove
              </button>
            </>
          ) : (
            <>
              {/* Single-Select Context Menu */}
              {contextItem.status === 'completed' && (
                <button
                  onClick={async () => {
                    setContextMenu(null);
                    await openDownloadFile(contextItem);
                  }}
                  className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
                >
                  Open
                </button>
              )}

              <button
                onClick={async () => {
                  setContextMenu(null);
                  await revealDownloadFile(contextItem);
                }}
                className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
              >
                Show in Finder
              </button>

              <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

              {canPauseDownload(contextItem.status) && (
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

              {canStartDownload(contextItem.status) && (
                <button
                  onClick={() => {
                    setContextMenu(null);
                    handleResume(contextItem);
                  }}
                  className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
                >
                  {startActionLabel(contextItem.status)}
                </button>
              )}

              {canRedownload(contextItem.status) && (
                <button
                  onClick={async () => {
                    setContextMenu(null);
                    try {
                      await redownload(contextItem.id);
                    } catch (error) {
                      showInteractionError('Redownload failed', error);
                    }
                  }}
                  className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
                >
                  Redownload
                </button>
              )}

              {contextItem.status !== 'completed' && (
                <div className="group relative">
                  <button className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors flex justify-between items-center">
                    Add to Queue
                    <ChevronRight size={14} />
                  </button>
                  <div className="absolute left-full top-0 hidden group-hover:block ml-1 min-w-[150px] bg-bg-modal border border-border-modal rounded-lg shadow-lg py-1.5 z-50">
                    {queues.map(q => (
                      <button key={q.id} onClick={() => {
                        setContextMenu(null);
                        assignToQueue([contextItem.id], q.id);
                      }} className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors text-[12px]">
                        {q.name}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

              <button
                onClick={() => {
                  setContextMenu(null);
                  navigator.clipboard.writeText(contextItem.url).catch(error => {
                    showInteractionError('Could not copy address', error);
                  });
                }}
                className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
              >
                Copy Address
              </button>

              {contextItem.status === 'completed' && (
                <button
                  onClick={async () => {
                    setContextMenu(null);
                    const fullPath = await getDownloadPath(contextItem);
                    if (!fullPath) {
                      showInteractionError('Could not copy file path', 'File name is missing');
                      return;
                    }
                    try {
                      await navigator.clipboard.writeText(fullPath);
                    } catch (error) {
                      showInteractionError('Could not copy file path', error);
                    }
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
                  openProperties(contextItem.id);
                }}
                className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
              >
                Properties
              </button>
            </>
          )}
        </div>
      )}

    </div>
  );
};
