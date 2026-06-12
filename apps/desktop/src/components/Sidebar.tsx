import React from 'react';
import {
  Inbox, Zap, CheckCircle2, CircleDashed,
  Film, Music, FileText, Box, Image as ImageIcon, Archive, FileQuestion,
  List, CalendarClock, Gauge, Settings, Plus
} from 'lucide-react';
import { useDownloadStore, DownloadCategory } from '../store/useDownloadStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { WindowDragRegion } from './WindowDragRegion';

export type SidebarFilter = 'all' | 'active' | 'completed' | 'unfinished' | DownloadCategory | 'settings';

interface SidebarProps {
  selectedFilter: SidebarFilter;
  onSelectFilter: (filter: SidebarFilter) => void;
}

export const Sidebar: React.FC<SidebarProps> = (props) => {
  const selectedFilter = props.selectedFilter;
  const onSelectFilter = props.onSelectFilter;
  const downloads = useDownloadStore(state => state.downloads);
  const activeView = useSettingsStore(state => state.activeView);

  const getCount = (filter: SidebarFilter) => {
    switch (filter) {
      case 'all': return downloads.length;
      case 'active': return downloads.filter(d => d.status === 'downloading').length;
      case 'completed': return downloads.filter(d => d.status === 'completed').length;
      case 'unfinished': return downloads.filter(d => d.status !== 'completed').length;
      default: return downloads.filter(d => d.category === filter).length;
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
          <div className="flex items-center px-2.5 py-1.5 rounded-lg text-[13px] text-text-primary hover:bg-item-hover cursor-default transition-colors mb-0.5">
            <List className="w-4 h-4 mr-2 text-text-secondary" strokeWidth={2} />
            <span>Main Queue</span>
          </div>
          <div className="flex items-center px-2.5 py-1.5 rounded-lg text-[13px] text-text-secondary hover:bg-item-hover cursor-default transition-colors">
            <Plus className="w-4 h-4 mr-2" strokeWidth={2} />
            <span>Add new queue</span>
          </div>
        </section>

        <section>
          <div className="text-[11px] font-semibold text-text-muted px-2.5 mb-1.5">Tools</div>
          <div className="flex items-center px-2.5 py-1.5 rounded-lg text-[13px] text-text-primary hover:bg-item-hover cursor-default transition-colors mb-0.5">
            <CalendarClock className="w-4 h-4 mr-2 text-text-secondary" strokeWidth={2} /><span>Scheduler</span>
          </div>
          <div className="flex items-center px-2.5 py-1.5 rounded-lg text-[13px] text-text-primary hover:bg-item-hover cursor-default transition-colors">
            <Gauge className="w-4 h-4 mr-2 text-text-secondary" strokeWidth={2} /><span>Speed Limiter</span>
          </div>
        </section>
      </div>

      <div className="shrink-0 border-t border-border-color bg-sidebar-glass px-2 py-2 backdrop-blur-xl">
        <button
          type="button"
          onClick={() => useSettingsStore.getState().setActiveView('settings')}
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
    </aside>
  );
};
