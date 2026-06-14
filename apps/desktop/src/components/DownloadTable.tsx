import React, { useState, useEffect } from 'react';
import { useDownloadStore, DownloadItem } from '../store/useDownloadStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { SidebarFilter } from './Sidebar';
import { Play, Pause, Plus, Trash2, FileText, Image as ImageIcon, Music, Film, Box, Archive, FileQuestion, MoreVertical, PanelLeft } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { homeDir } from '@tauri-apps/api/path';

interface DownloadTableProps {
  filter: SidebarFilter;
}

export const DownloadTable: React.FC<DownloadTableProps> = ({ filter }) => {
  const { downloads, toggleAddModal, updateDownload, removeDownload, clearFinished, redownload } = useDownloadStore();
  const { isSidebarVisible, toggleSidebar } = useSettingsStore();

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; id: string } | null>(null);

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
    return resolvedDir + '/' + file;
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
      updateDownload(id, { status: 'paused', speed: '-', eta: '-' });
    } catch (e) {
      console.error("Failed to pause:", e);
    }
  };

  const handleResume = (item: DownloadItem) => {
    useDownloadStore.setState((state) => ({
      downloads: state.downloads.map(d => d.id === item.id ? { ...d, status: 'queued', speed: '-', eta: '-' } : d)
    }));
    useDownloadStore.getState().processQueue();
  };

  const handleDelete = async (id: string) => {
    try {
      await removeDownload(id);
    } catch (e) {
      console.error("Failed to delete download:", e);
    }
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

          <button 
            className="main-control-button hover:!text-red-400" 
            disabled={filteredDownloads.length === 0}
            onClick={clearFinished}
            title="Clear Finished"
          >
            <Trash2 size={15} />
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
        <div className="download-table-header">
          <div>File Name</div>
          <div>Size</div>
          <div>Status</div>
          <div>Speed</div>
          <div>ETA</div>
          <div className="download-cell-right">Date Added</div>
        </div>

        <div className="download-table-body">
          {filteredDownloads.length === 0 ? (
            <div className="h-full overflow-auto">
              <div className="download-row download-empty-row">
                <div className="download-file-cell">
                  <FileQuestion size={15} />
                  <span className="download-file-name">
                    No downloads yet · Use + to add a link
                  </span>
                </div>
                <div>—</div>
                <div>Idle</div>
                <div>—</div>
                <div>—</div>
                <div className="download-cell-right">—</div>
              </div>

              {Array.from({ length: 12 }).map((_, index) => (
                <div key={index} className="download-ghost-row" />
              ))}
            </div>
          ) : (
            <div className="h-full overflow-auto">
              {filteredDownloads.map(d => (
                <div
                  key={d.id}
                  className="download-row group cursor-default relative"
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setContextMenu({ x: e.clientX, y: e.clientY, id: d.id });
                  }}
                >
                  <div className="download-file-cell">
                    <span className="shrink-0 text-text-muted">
                      {getCategoryIcon(d.category)}
                    </span>
                    <span className="download-file-name">
                      {d.fileName}
                    </span>
                  </div>
                  
                  <div>
                    <span className="tabular-nums">
                      {d.status === 'downloading' || d.status === 'paused' 
                        ? `${((d.fraction || 0) * parseInt((d.size || '').replace(/[^0-9.]/g, '') || '0')).toFixed(1)} GB / ${d.size || '-'}`
                        : d.size || '-'}
                    </span>
                  </div>
                  
                  <div>
                    <div className="flex flex-col justify-center gap-1.5">
                      <div className="download-progress-status">
                        <span className={`truncate text-[12px] ${d.status === 'completed' ? 'download-status-completed' : d.status === 'paused' ? 'download-status-paused' : d.status === 'failed' ? 'download-status-failed' : 'download-status-downloading'}`}>
                          {d.status === 'downloading' || d.status === 'paused' ? `${((d.fraction || 0) * 100).toFixed(0)}%` : d.status.charAt(0).toUpperCase() + d.status.slice(1)}
                        </span>
                      </div>
                      {(d.status === 'downloading' || d.status === 'paused') && (
                        <div className="download-progress-track">
                          <div className={`download-progress-fill transition-all duration-300 ease-out ${d.status === 'paused' ? 'paused' : ''}`} style={{ width: `${(d.fraction || 0) * 100}%` }}></div>
                        </div>
                      )}
                    </div>
                  </div>
                  
                  <div>
                    <span className="tabular-nums">{d.status === 'downloading' ? d.speed : '-'}</span>
                  </div>
                  
                  <div>
                    <span className="tabular-nums">{d.status === 'downloading' ? d.eta : '-'}</span>
                  </div>
                  
                  <div className="download-cell-right">
                    <span className="truncate group-hover:hidden tabular-nums ml-auto">
                      {d.dateAdded ? new Date(d.dateAdded).toLocaleDateString() : '-'}
                    </span>
                    
                    <div className="hidden group-hover:flex items-center justify-end gap-0.5 w-full ml-auto">
                      {d.status === 'downloading' && (
                        <button onClick={() => handlePause(d.id)} className="app-icon-button h-7 w-7" title="Pause">
                          <Pause size={14} fill="currentColor" />
                        </button>
                      )}
                      {d.status === 'paused' && (
                        <button onClick={() => handleResume(d)} className="app-icon-button h-7 w-7" title="Resume">
                          <Play size={14} fill="currentColor" />
                        </button>
                      )}
                      <button
                        onClick={(e) => {
                           e.stopPropagation();
                           setContextMenu({ x: e.clientX, y: e.clientY, id: d.id });
                        }}
                        className="app-icon-button h-7 w-7"
                        title="Options"
                      >
                        <MoreVertical size={14} />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
              {Array.from({ length: Math.max(0, 10 - filteredDownloads.length) }).map((_, index) => (
                <div key={`ghost-${index}`} className="download-ghost-row" />
              ))}
            </div>
          )}
        </div>
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
                  await invoke('open_file', { path: fullPath });
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
                await invoke('show_in_folder', { path: fullPath });
              } catch (e) {
                console.error("Failed to show in folder:", e);
              }
            }}
            className="w-full text-left px-3 py-2 hover:bg-item-hover transition-colors"
          >
            Show in Finder
          </button>

          <div className="h-[1px] bg-border-modal/60 my-1.5 mx-2"></div>

          {(contextItem.status === 'downloading' || contextItem.status === 'queued') && (
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

          {(contextItem.status === 'paused' || contextItem.status === 'failed') && (
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

          {['completed', 'failed', 'paused'].includes(contextItem.status) && (
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
            Remove from List
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
