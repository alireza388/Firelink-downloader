import React, { useState, useEffect, useRef } from 'react';
import {
  Inbox, Zap, CheckCircle2, CircleDashed,
  Film, Music, FileText, Box, Image as ImageIcon, Archive, FileQuestion,
  List, CalendarClock, Gauge, Settings, Plus, Play, Pause, Edit2, Trash2
} from 'lucide-react';
import { useDownloadStore, DownloadCategory, Queue } from '../store/useDownloadStore';
import { ActiveView, useSettingsStore } from '../store/useSettingsStore';
import { WindowDragRegion } from './WindowDragRegion';

export type SidebarFilter = 'all' | 'active' | 'completed' | 'unfinished' | DownloadCategory | 'settings' | string;

interface SidebarProps {
  selectedFilter: SidebarFilter;
  onSelectFilter: (filter: SidebarFilter) => void;
}

export const Sidebar: React.FC<SidebarProps> = (props) => {
  const { selectedFilter, onSelectFilter } = props;
  const { downloads, queues, addQueue, renameQueue, removeQueue, startQueue, pauseQueue } = useDownloadStore();
  const { activeView, setActiveView } = useSettingsStore();

  const [isAddingQueue, setIsAddingQueue] = useState(false);
  const [newQueueName, setNewQueueName] = useState('');
  const [renamingQueueId, setRenamingQueueId] = useState<string | null>(null);
  const [editingQueueName, setEditingQueueName] = useState('');
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; id: string } | null>(null);

  const addInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const handleCloseMenu = () => setContextMenu(null);
    window.addEventListener('click', handleCloseMenu);
    return () => window.removeEventListener('click', handleCloseMenu);
  }, []);

  useEffect(() => {
    if (isAddingQueue) addInputRef.current?.focus();
  }, [isAddingQueue]);

  useEffect(() => {
    if (renamingQueueId) renameInputRef.current?.focus();
  }, [renamingQueueId]);

  const getCount = (filter: SidebarFilter) => {
    if (filter.startsWith('queue:')) {
      const qid = filter.replace('queue:', '');
      return downloads.filter(d => d.queueId === qid).length;
    }
    switch (filter) {
      case 'all': return downloads.length;
      case 'active': return downloads.filter(d => d.status === 'downloading').length;
      case 'completed': return downloads.filter(d => d.status === 'completed').length;
      case 'unfinished': return downloads.filter(d => d.status !== 'completed').length;
      default: return downloads.filter(d => d.category === filter as DownloadCategory).length;
    }
  };

  const NavItem = ({ icon: Icon, label, filter }: { icon: any, label: string, filter: SidebarFilter }) => {
    const isSelected = activeView === 'downloads' && selectedFilter === filter;

    return (
      <button
        type="button"
        className={`group flex w-full items-center px-2.5 py-1.5 rounded-lg text-[13px] text-left cursor-default transition-colors mb-0.5 ${
          isSelected
            ? 'bg-accent text-white font-medium'
            : 'text-text-primary hover:bg-item-hover'
        }`}
        onClick={() => onSelectFilter(filter)}
      >
        <Icon className={`w-4 h-4 mr-2 ${isSelected ? 'text-white' : 'text-text-secondary'}`} strokeWidth={isSelected ? 2.25 : 2} />
        <span className="truncate">{label}</span>
        {getCount(filter) > 0 && (
          <span className={`ml-auto min-w-5 px-1.5 py-0.5 rounded-full text-center text-[10px] leading-none font-semibold ${
            isSelected ? 'bg-black/20 text-white' : 'bg-item-hover text-text-secondary'
          }`}>
            {getCount(filter)}
          </span>
        )}
      </button>
    );
  };

  const handleQueueContextMenu = (e: React.MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, id });
  };

  const handleAddQueueSubmit = () => {
    if (newQueueName.trim()) addQueue(newQueueName.trim());
    setNewQueueName('');
    setIsAddingQueue(false);
  };

  const handleRenameQueueSubmit = () => {
    if (renamingQueueId && editingQueueName.trim()) {
      renameQueue(renamingQueueId, editingQueueName.trim());
    }
    setRenamingQueueId(null);
  };

  const QueueItem = ({ queue }: { queue: Queue }) => {
    const filterId = `queue:${queue.id}`;
    const isSelected = activeView === 'downloads' && selectedFilter === filterId;
    const isRenaming = renamingQueueId === queue.id;

    if (isRenaming) {
      return (
        <div className="flex items-center px-2.5 py-1 rounded-lg mb-0.5 bg-item-hover">
          <List className="w-4 h-4 mr-2 text-text-secondary" strokeWidth={2} />
          <input
            ref={renameInputRef}
            type="text"
            className="flex-1 bg-transparent border border-accent rounded px-1 text-[13px] text-text-primary outline-none min-w-0"
            value={editingQueueName}
            onChange={e => setEditingQueueName(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter') handleRenameQueueSubmit();
              if (e.key === 'Escape') setRenamingQueueId(null);
            }}
            onBlur={handleRenameQueueSubmit}
          />
        </div>
      );
    }

    return (
      <button
        type="button"
        onContextMenu={e => handleQueueContextMenu(e, queue.id)}
        onClick={() => onSelectFilter(filterId)}
        className={`group flex w-full items-center px-2.5 py-1.5 rounded-lg text-[13px] text-left cursor-default transition-colors mb-0.5 ${
          isSelected
            ? 'bg-accent text-white font-medium'
            : 'text-text-primary hover:bg-item-hover'
        }`}
      >
        <List className={`w-4 h-4 mr-2 shrink-0 ${isSelected ? 'text-white' : 'text-text-secondary'}`} strokeWidth={isSelected ? 2.25 : 2} />
        <span className="truncate">{queue.name}</span>
        {getCount(filterId) > 0 && (
          <span className={`ml-auto min-w-5 px-1.5 py-0.5 rounded-full text-center text-[10px] leading-none font-semibold shrink-0 ${
            isSelected ? 'bg-black/20 text-white' : 'bg-item-hover text-text-secondary'
          }`}>
            {getCount(filterId)}
          </span>
        )}
      </button>
    );
  };

  const ToolItem = ({ icon: Icon, label, view }: { icon: any; label: string; view: ActiveView }) => {
    const isSelected = activeView === view;
    return (
      <button
        type="button"
        onClick={() => setActiveView(view)}
        className={`flex w-full items-center px-2.5 py-1.5 rounded-lg text-[13px] text-left cursor-default transition-colors mb-0.5 ${
          isSelected ? 'bg-accent text-white font-medium' : 'text-text-primary hover:bg-item-hover'
        }`}
      >
        <Icon className={`w-4 h-4 mr-2 ${isSelected ? 'text-white' : 'text-text-secondary'}`} strokeWidth={isSelected ? 2.25 : 2} />
        <span>{label}</span>
      </button>
    );
  };

  return (
    <aside className="w-[220px] min-w-[190px] max-w-[260px] bg-sidebar-bg border-r border-border-color flex flex-col relative shrink-0">
      <WindowDragRegion />
      <div className="overflow-y-auto flex-1 px-2 pb-3">
        <section className="mb-4">
          <div className="text-[11px] font-semibold text-text-muted px-2.5 mb-1.5">Library</div>
          <NavItem icon={Inbox} label="All" filter="all" />
          <NavItem icon={Zap} label="Active" filter="active" />
          <NavItem icon={CheckCircle2} label="Completed" filter="completed" />
          <NavItem icon={CircleDashed} label="Unfinished" filter="unfinished" />
        </section>

        <section className="mb-4">
          <div className="text-[11px] font-semibold text-text-muted px-2.5 mb-1.5">Folders</div>
          <NavItem icon={Film} label="Video" filter="Video" />
          <NavItem icon={Music} label="Audio" filter="Audio" />
          <NavItem icon={FileText} label="Documents" filter="Documents" />
          <NavItem icon={Box} label="Apps" filter="Apps" />
          <NavItem icon={ImageIcon} label="Images" filter="Images" />
          <NavItem icon={Archive} label="Archives" filter="Archives" />
          <NavItem icon={FileQuestion} label="Other" filter="Other" />
        </section>

        <section className="mb-4">
          <div className="text-[11px] font-semibold text-text-muted px-2.5 mb-1.5">Queues</div>
          {queues.map(queue => (
            <QueueItem key={queue.id} queue={queue} />
          ))}
          {isAddingQueue ? (
            <div className="flex items-center px-2.5 py-1 rounded-lg bg-item-hover">
              <Plus className="w-4 h-4 mr-2 text-text-secondary shrink-0" strokeWidth={2} />
              <input
                ref={addInputRef}
                type="text"
                placeholder="Queue name"
                className="flex-1 bg-transparent border border-accent rounded px-1 text-[13px] text-text-primary outline-none min-w-0"
                value={newQueueName}
                onChange={e => setNewQueueName(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter') handleAddQueueSubmit();
                  if (e.key === 'Escape') setIsAddingQueue(false);
                }}
                onBlur={handleAddQueueSubmit}
              />
            </div>
          ) : (
            <button
              type="button"
              onClick={() => { setIsAddingQueue(true); setNewQueueName(''); }}
              className="flex w-full items-center px-2.5 py-1.5 rounded-lg text-[13px] text-text-secondary hover:bg-item-hover cursor-default transition-colors"
            >
              <Plus className="w-4 h-4 mr-2 shrink-0" strokeWidth={2} />
              <span className="truncate">Add new queue</span>
            </button>
          )}
        </section>

        <section>
          <div className="text-[11px] font-semibold text-text-muted px-2.5 mb-1.5">Tools</div>
          <ToolItem icon={CalendarClock} label="Scheduler" view="scheduler" />
          <ToolItem icon={Gauge} label="Speed Limiter" view="speedLimiter" />
        </section>
      </div>

      <div className="shrink-0 border-t border-border-color bg-sidebar-glass px-2 py-2 backdrop-blur-xl">
        <button
          type="button"
          onClick={() => setActiveView('settings')}
          className={`flex w-full items-center px-2.5 py-2 rounded-lg text-[13px] text-left cursor-default transition-colors ${
            activeView === 'settings'
              ? 'bg-accent text-white font-medium'
              : 'text-text-primary hover:bg-item-hover'
          }`}
        >
          <Settings className={`w-4 h-4 mr-2 ${activeView === 'settings' ? 'text-white' : 'text-text-secondary'}`} strokeWidth={activeView === 'settings' ? 2.25 : 2} />
          <span>Settings</span>
        </button>
      </div>

      {contextMenu && (
        <div
          className="fixed z-50 w-48 py-1 rounded-xl shadow-lg border border-border-modal bg-bg-context-menu backdrop-blur-xl animate-fade-in text-[13px] text-text-primary overflow-hidden"
          style={{ top: contextMenu.y, left: contextMenu.x }}
          onClick={e => e.stopPropagation()}
        >
          <button
            className="w-full text-left px-3 py-1.5 flex items-center hover:bg-item-hover"
            onClick={() => { startQueue(contextMenu.id); setContextMenu(null); }}
          >
            <Play size={14} className="mr-2 text-text-secondary" />
            Start Queue
          </button>
          <button
            className="w-full text-left px-3 py-1.5 flex items-center hover:bg-item-hover"
            onClick={() => { pauseQueue(contextMenu.id); setContextMenu(null); }}
          >
            <Pause size={14} className="mr-2 text-text-secondary" />
            Pause Queue
          </button>
          <div className="h-px bg-border-color my-1 mx-2" />
          <button
            className="w-full text-left px-3 py-1.5 flex items-center hover:bg-item-hover"
            onClick={() => {
              const q = queues.find(q => q.id === contextMenu.id);
              if (q) {
                setEditingQueueName(q.name);
                setRenamingQueueId(q.id);
              }
              setContextMenu(null);
            }}
          >
            <Edit2 size={14} className="mr-2 text-text-secondary" />
            Rename Queue
          </button>
          {!queues.find(q => q.id === contextMenu.id)?.isMain && (
            <button
              className="w-full text-left px-3 py-1.5 flex items-center hover:bg-red-500/20 text-red-400"
              onClick={() => { removeQueue(contextMenu.id); setContextMenu(null); }}
            >
              <Trash2 size={14} className="mr-2" />
              Delete Queue
            </button>
          )}
        </div>
      )}
    </aside>
  );
};
