import { useState, useEffect, useRef } from 'react';
import {
  useDownloadStore,
  getSiteLogin,
  type AddDownloadAction
} from '../store/useDownloadStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { FolderPlus, Settings, Shield, RefreshCw, FileText, HardDrive, Database, Link, ArrowRight, Play, ChevronDown, ChevronRight, Video, Film, Music, type LucideIcon } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { invokeCommand as invoke } from '../ipc';
import { DuplicateResolutionModal, DuplicateConflict } from './DuplicateResolutionModal';
import { canonicalizeDownloadFileName, categoryForFileName } from '../utils/downloads';
import { fetchMediaMetadataDeduped } from '../utils/mediaMetadata';
import {
  resolveCategoryDestination,
  resolveDownloadFilePath,
  downloadLocationEquals
} from '../utils/downloadLocations';
import { getPlatformInfo } from '../utils/platform';
import { isTransferLocked } from '../utils/downloadActions';
import { useToast } from '../contexts/ToastContext';
import {
  canSubmitMetadataRows,
  mediaFormatSelectorForRow,
  metadataSummaryMessage,
  reconcileDownloadRows,
  refreshFailedMetadataRows,
  updateRowIfCurrent,
  type AddDownloadDraftRow
} from '../utils/addDownloadMetadata';

const formatBytes = (bytes: number) => {
  if (bytes === 0) return 'Unknown size';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

export const AddDownloadsModal = () => {
  const { addToast } = useToast();
  const {
    isAddModalOpen,
    pendingAddUrls,
    pendingAddReferer,
    pendingAddFilename,
    pendingAddHeaders,
    pendingAddCookies,
    toggleAddModal,
    addDownload,
    queues
  } = useDownloadStore();
  const { baseDownloadFolder, perServerConnections } = useSettingsStore();

  const [urls, setUrls] = useState('');
  const [selectedItemIndex, setSelectedItemIndex] = useState<number | null>(null);
  const [parsedItems, setParsedItems] = useState<AddDownloadDraftRow[]>([]);
  const metadataRequestsRef = useRef(new Set<string>());

  const [conflicts, setConflicts] = useState<DuplicateConflict[]>([]);
  const [showingDuplicates, setShowingDuplicates] = useState(false);
  const [pendingAction, setPendingAction] = useState<AddDownloadAction>({ type: 'start-now' });
  const [pendingUseSharedDestination, setPendingUseSharedDestination] = useState(false);
  const [pendingDestinationOverrides, setPendingDestinationOverrides] = useState<Record<number, string>>({});
  const [resolvedLocation, setResolvedLocation] = useState('');
  const [isQueueMenuOpen, setIsQueueMenuOpen] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const actionMenuRef = useRef<HTMLDivElement>(null);

  // Right Form
  const [saveLocation, setSaveLocation] = useState(baseDownloadFolder);
  const [isSaveLocationManual, setIsSaveLocationManual] = useState(false);
  const [connections, setConnections] = useState(perServerConnections);
  const [speedLimitEnabled, setSpeedLimitEnabled] = useState(false);
  const [speedLimit, setSpeedLimit] = useState('1024');
  const [freeSpace, setFreeSpace] = useState('Unknown');

  const [useAuth, setUseAuth] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');

  const [advancedExpanded, setAdvancedExpanded] = useState(false);
  const [checksumEnabled, setChecksumEnabled] = useState(false);
  const [checksumAlgo, setChecksumAlgo] = useState('SHA-256');
  const [checksumValue, setChecksumValue] = useState('');
  const [headers, setHeaders] = useState('');
  const [cookies, setCookies] = useState('');
  const [mirrors, setMirrors] = useState('');

  useEffect(() => {
    if (isAddModalOpen) {
      setSaveLocation(baseDownloadFolder);
      setIsSaveLocationManual(false);
      setUrls(pendingAddUrls || '');
      setParsedItems([]);
      setSelectedItemIndex(null);
      setPendingUseSharedDestination(false);
      setPendingDestinationOverrides({});
      setConnections(perServerConnections);
      setSpeedLimitEnabled(false);
      setSpeedLimit('1024');
      setUseAuth(false);
      setUsername('');
      setPassword('');
      setAdvancedExpanded(false);
      setChecksumEnabled(false);
      setChecksumAlgo('SHA-256');
      setChecksumValue('');
      setHeaders([
        pendingAddReferer ? `Referer: ${pendingAddReferer.replace(/[\r\n]/g, '')}` : '',
        pendingAddHeaders
      ].filter(Boolean).join('\n'));
      setCookies(pendingAddCookies);
      setMirrors('');
      setIsQueueMenuOpen(false);
      setIsSubmitting(false);
    } else {
      setUrls('');
    }
  }, [
    isAddModalOpen,
    pendingAddUrls,
    pendingAddReferer,
    pendingAddHeaders,
    pendingAddCookies,
    baseDownloadFolder,
    perServerConnections
  ]);

  useEffect(() => {
    if (!isQueueMenuOpen) return;
    const closeMenu = (event: PointerEvent) => {
      if (!actionMenuRef.current?.contains(event.target as Node)) {
        setIsQueueMenuOpen(false);
      }
    };
    window.addEventListener('pointerdown', closeMenu);
    return () => window.removeEventListener('pointerdown', closeMenu);
  }, [isQueueMenuOpen]);

  useEffect(() => {
    if (!isQueueMenuOpen && !showingDuplicates) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      if (showingDuplicates) {
        setShowingDuplicates(false);
      } else {
        setIsQueueMenuOpen(false);
      }
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [isQueueMenuOpen, showingDuplicates]);

  useEffect(() => {
    if (!saveLocation) return;
    invoke('get_free_space', { path: saveLocation })
      .then(space => setFreeSpace(space))
      .catch(() => setFreeSpace('Unknown'));
  }, [saveLocation, isAddModalOpen]);

  useEffect(() => {
    setParsedItems(current =>
      reconcileDownloadRows(urls, current, pendingAddFilename || undefined)
    );
  }, [urls, pendingAddFilename]);

  useEffect(() => {
    for (const row of parsedItems) {
      if (row.status !== 'loading') continue;
      const requestKey = `${row.id}:${row.generation}`;
      if (metadataRequestsRef.current.has(requestKey)) continue;
      metadataRequestsRef.current.add(requestKey);

      void (async () => {
        try {
          if (row.isMedia) {
            const settingsStore = useSettingsStore.getState();
            const { mediaCookieSource } = settingsStore;
            const browserArg = mediaCookieSource !== 'none' ? mediaCookieSource : null;
            const login = getSiteLogin(row.sourceUrl, settingsStore);
            let keychainPassword = null;
            if (login) {
              try {
                keychainPassword = await invoke('get_keychain_password', { id: login.id });
              } catch (e) {
                console.warn("Could not fetch keychain password:", e);
              }
            }

            const mediaData = await fetchMediaMetadataDeduped({
              url: row.sourceUrl,
              cookieBrowser: browserArg,
              username: useAuth ? username.trim() || null : login?.username || null,
              password: useAuth ? password || null : keychainPassword
            });
            if (mediaData && mediaData.formats.length > 0) {
              const mappedFormats = mediaData.formats.map(f => {
                const quality = f.resolution || 'Video';
                const container = f.ext.toUpperCase();
                const exactBytes = f.filesize || 0;
                const approxBytes = f.filesize_approx || 0;
                const bytes = exactBytes || approxBytes;
                const isApproximate = !exactBytes && approxBytes > 0;
                return {
                  name: `${quality} ${container}`,
                  ext: f.ext,
                  bytes,
                  isApproximate,
                  formatLabel: f.format_label || f.ext.toUpperCase(),
                  detail: bytes ? `${isApproximate ? '~' : ''}${formatBytes(bytes)}` : 'Unknown size',
                  selector: f.format_id,
                  type: quality.toLowerCase().includes('audio') ? 'Audio' : 'Video'
                };
              });
              setParsedItems(current => updateRowIfCurrent(
                current,
                row.id,
                row.sourceUrl,
                row.generation,
                currentRow => ({
                  ...currentRow,
                  downloadUrl: row.sourceUrl,
                  file: canonicalizeDownloadFileName(`${mediaData.title}.${mediaData.formats[0].ext}`),
                  size: mappedFormats[0].bytes ? mappedFormats[0].detail : undefined,
                  sizeBytes: mappedFormats[0].bytes || undefined,
                  status: 'ready',
                  formats: mappedFormats,
                  selectedFormat: 0
                })
              ));
            } else {
              throw new Error("Invalid media metadata or no formats found");
            }
          } else {
            const settingsStore = useSettingsStore.getState();
            const login = getSiteLogin(row.sourceUrl, settingsStore);
            let keychainPassword = null;
            if (login) {
              try {
                keychainPassword = await invoke('get_keychain_password', { id: login.id });
              } catch (e) {
                console.warn("Could not fetch keychain password:", e);
              }
            }
            const meta = await invoke('fetch_metadata', {
              url: row.sourceUrl,
              userAgent: settingsStore.customUserAgent || null,
              username: useAuth ? username.trim() || null : login?.username || null,
              password: useAuth ? password || null : keychainPassword
            });
            setParsedItems(current => updateRowIfCurrent(
              current,
              row.id,
              row.sourceUrl,
              row.generation,
              currentRow => ({
                ...currentRow,
                downloadUrl: meta.url || currentRow.downloadUrl,
                file: canonicalizeDownloadFileName(
                  current.length === 1 && pendingAddFilename
                    ? pendingAddFilename
                    : meta.filename
                ),
                size: meta.size_bytes ? meta.size : undefined,
                sizeBytes: meta.size_bytes || undefined,
                status: 'ready'
              })
            ));
          }
        } catch (e) {
          console.error("Meta fetch failed", e);
          setParsedItems(current => updateRowIfCurrent(
            current,
            row.id,
            row.sourceUrl,
            row.generation,
            currentRow => ({
              ...currentRow,
              downloadUrl: currentRow.sourceUrl,
              size: undefined,
              sizeBytes: undefined,
              status: 'metadata-error',
              formats: undefined,
              selectedFormat: undefined
            })
          ));
        } finally {
          metadataRequestsRef.current.delete(requestKey);
        }
      })();
    }
  }, [parsedItems, pendingAddFilename, password, useAuth, username]);

  useEffect(() => {
    if (parsedItems.length === 0) {
      setSelectedItemIndex(null);
      return;
    }
    setSelectedItemIndex(current =>
      current === null || current >= parsedItems.length ? 0 : current
    );
    if (isSaveLocationManual) return;
    if (parsedItems.length > 1) {
      setSaveLocation(useSettingsStore.getState().baseDownloadFolder || '~/Downloads');
      return;
    }
    const first = parsedItems[0];
    if (first.status !== 'ready' && first.status !== 'metadata-error') return;
    void resolveCategoryDestination(
      useSettingsStore.getState(),
      categoryForFileName(first.file)
    ).then(setSaveLocation);
  }, [isSaveLocationManual, parsedItems]);

  if (!isAddModalOpen) return null;

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: saveLocation.startsWith('~') ? undefined : saveLocation
    });
    if (selected && typeof selected === 'string') {
      const approvedPath = await useSettingsStore.getState().approveDownloadRoot(selected);
      setSaveLocation(approvedPath);
      setIsSaveLocationManual(true);
    }
    } catch (e) {
      console.error("Failed to select folder:", e);
    }
  };

  const categoryLocationForFile = (fileName: string) => {
    const category = categoryForFileName(fileName);
    return resolveCategoryDestination(useSettingsStore.getState(), category);
  };

  const handleAction = async (action: AddDownloadAction) => {
    if (isSubmitting || !canSubmitMetadataRows(parsedItems)) {
      return;
    }
    if (speedLimitEnabled && (!Number.isFinite(Number(speedLimit)) || Number(speedLimit) <= 0)) {
      addToast({ message: 'Speed limit must be greater than zero', variant: 'error', isActionable: true });
      return;
    }
    setIsSubmitting(true);
    let finalLocation = saveLocation;
    let useSharedDestination = isSaveLocationManual;
    const destinationOverrides: Record<number, string> = {};
    const settings = useSettingsStore.getState();
    const platform = await getPlatformInfo().catch(() => ({ os: 'unknown' }));
    if (settings.askWhereToSaveEachFile && parsedItems.length > 0) {
      for (const [index, item] of parsedItems.entries()) {
        try {
          const suggestedLocation = isSaveLocationManual
            ? finalLocation
            : await categoryLocationForFile(item.file);
          const selected = await open({
            directory: true,
            multiple: false,
            title: `Choose a folder for ${item.file}`,
            defaultPath: suggestedLocation.startsWith('~') ? undefined : suggestedLocation
          });
          if (selected && typeof selected === 'string') {
            const approvedPath = await useSettingsStore.getState().approveDownloadRoot(selected);
            destinationOverrides[index] = approvedPath;
          } else {
            setIsSubmitting(false);
            return;
          }
        } catch (e) {
          console.error("Failed to select folder:", e);
          setIsSubmitting(false);
          return;
        }
      }
    }

    setResolvedLocation(finalLocation);
    const store = useDownloadStore.getState();
    const newConflicts: DuplicateConflict[] = [];

    for (let i = 0; i < parsedItems.length; i++) {
      const item = parsedItems[i];
      let finalFile = canonicalizeDownloadFileName(item.file);
      if (item.isMedia && item.formats && item.selectedFormat !== undefined) {
        const selectedFormat = item.formats[item.selectedFormat];
        const baseName = finalFile.substring(0, finalFile.lastIndexOf('.')) || finalFile;
        finalFile = `${baseName}.${selectedFormat.ext}`;
      }
      const itemLocation = useSharedDestination
        ? finalLocation
        : destinationOverrides[i] || await categoryLocationForFile(finalFile);

      const isUrlDupe = store.downloads.some(d => d.url === item.downloadUrl && d.status !== 'failed' && d.status !== 'completed');
      if (isUrlDupe) {
        newConflicts.push({ id: i.toString(), fileName: finalFile, reason: { type: 'url', msg: 'URL already in queue' }, resolution: 'rename' });
      } else {
        let fileExistsInStore = false;
        for (const download of store.downloads) {
          const destination = download.destination ||
            await resolveCategoryDestination(settings, download.category);
          if (
            downloadLocationEquals(
              destination,
              download.fileName,
              itemLocation,
              finalFile,
              platform.os
            ) &&
            download.status !== 'failed'
          ) {
            fileExistsInStore = true;
            break;
          }
        }

        let fileExistsOnDisk = false;
        try {
          fileExistsOnDisk = await invoke('check_file_exists', {
            path: await resolveDownloadFilePath(itemLocation, finalFile)
          });
        } catch (e) {
          console.error("Failed to check if file exists on disk:", e);
        }

        if (fileExistsInStore || fileExistsOnDisk) {
          newConflicts.push({
            id: i.toString(),
            fileName: finalFile,
            reason: {
              type: 'file',
              msg: fileExistsInStore
                ? 'Existing Firelink download uses this destination'
                : 'File exists on disk; rename or skip to avoid deleting unrelated data'
            },
            resolution: 'rename',
            replaceAllowed: fileExistsInStore
          });
        }
      }
    }

    if (newConflicts.length > 0) {
      setConflicts(newConflicts);
      setPendingAction(action);
      setPendingUseSharedDestination(useSharedDestination);
      setPendingDestinationOverrides(destinationOverrides);
      setShowingDuplicates(true);
      setIsSubmitting(false);
      return;
    }

    try {
      await executeAddDownloads(action, finalLocation, useSharedDestination, undefined, destinationOverrides);
    } finally {
      setIsSubmitting(false);
    }
  };

  const executeAddDownloads = async (
    action: AddDownloadAction,
    finalLocation: string,
    useSharedDestination: boolean,
    resolutions?: { id: string, resolution: 'rename' | 'replace' | 'skip' }[],
    destinationOverrides: Record<number, string> = {}
  ) => {
      let itemsToAdd: Array<AddDownloadDraftRow | null> = [...parsedItems];
      const platform = await getPlatformInfo().catch(() => ({ os: 'unknown' }));

      if (resolutions) {
         for (const res of resolutions) {
             const idx = parseInt(res.id);
             const item = itemsToAdd[idx];
             if (!item) continue;
             const conflict = conflicts.find(c => c.id === res.id);

             if (res.resolution === 'skip') {
                 itemsToAdd[idx] = null;
             } else if (res.resolution === 'rename') {
                 let finalFile = canonicalizeDownloadFileName(item.file);
        if (item.isMedia && item.formats && item.selectedFormat !== undefined) {
          const selectedFormat = item.formats[item.selectedFormat];
          const baseName = finalFile.substring(0, finalFile.lastIndexOf('.')) || finalFile;
          finalFile = `${baseName}.${selectedFormat.ext}`;
        }
        const itemLocation = useSharedDestination
          ? finalLocation
          : destinationOverrides[idx] || await categoryLocationForFile(finalFile);
                 
                 let count = 1;
                 const base = finalFile.substring(0, finalFile.lastIndexOf('.')) || finalFile;
                 const ext = finalFile.includes('.') ? finalFile.substring(finalFile.lastIndexOf('.')) : '';
                 let newName = finalFile;
                 let exists = true;
                 
                 while (exists && count < 1000) {
          newName = `${base} (${count})${ext}`;
          let storeHas = false;
          const currentSettings = useSettingsStore.getState();
          for (const download of useDownloadStore.getState().downloads) {
            const destination = download.destination ||
              await resolveCategoryDestination(currentSettings, download.category);
            if (
              downloadLocationEquals(
                destination,
                download.fileName,
                itemLocation,
                newName,
                platform.os
              ) &&
              download.status !== 'failed'
            ) {
              storeHas = true;
              break;
            }
          }
                     let diskHas = false;
                     try {
                       diskHas = await invoke('check_file_exists', {
                         path: await resolveDownloadFilePath(itemLocation, newName)
                       });
                     } catch(e) {}
                     exists = storeHas || diskHas;
                     count++;
                 }
                 if (exists) {
                   throw new Error(`Could not find an available name for ${finalFile}.`);
                 }
                 
                 itemsToAdd[idx] = { ...item, file: newName };
             } else if (res.resolution === 'replace') {
              if (conflict?.reason.type !== 'file' || !conflict.replaceAllowed) {
                itemsToAdd[idx] = null;
                continue;
              }
              let finalFile = canonicalizeDownloadFileName(item.file);
        if (item.isMedia && item.formats && item.selectedFormat !== undefined) {
          const selectedFormat = item.formats[item.selectedFormat];
          const baseName = finalFile.substring(0, finalFile.lastIndexOf('.')) || finalFile;
          finalFile = `${baseName}.${selectedFormat.ext}`;
        }
        const itemLocation = useSharedDestination
          ? finalLocation
          : destinationOverrides[idx] || await categoryLocationForFile(finalFile);
        const store = useDownloadStore.getState();
        let existingItem;
        const currentSettings = useSettingsStore.getState();
        for (const download of store.downloads) {
          const destination = download.destination ||
            await resolveCategoryDestination(currentSettings, download.category);
          if (
            downloadLocationEquals(
              destination,
              download.fileName,
              itemLocation,
              finalFile,
              platform.os
            ) &&
            download.status !== 'failed'
          ) {
            existingItem = download;
            break;
          }
        }

                 if (existingItem && isTransferLocked(existingItem.status)) {
                   throw new Error(`Pause ${existingItem.fileName} before replacing it.`);
                 }

                 if (!existingItem) {
                   throw new Error(`Cannot replace ${finalFile}: file is not owned by a Firelink download.`);
                 }
                 await store.removeDownload(existingItem.id, true);
             }
         }
      }

      let addedCount = 0;
      const failures: string[] = [];

      for (const [itemIndex, item] of itemsToAdd.entries()) {
        if (!item) continue;
        try {
          const id = crypto.randomUUID();
          let finalFile = canonicalizeDownloadFileName(item.file);
          let formatSelector = mediaFormatSelectorForRow(item);

          if (item.isMedia && item.formats && item.selectedFormat !== undefined) {
            const selectedFormat = item.formats[item.selectedFormat];
            if (!finalFile.endsWith(`.${selectedFormat.ext}`)) {
                const baseName = finalFile.substring(0, finalFile.lastIndexOf('.')) || finalFile;
                finalFile = `${baseName}.${selectedFormat.ext}`;
            }
        }

        const category = categoryForFileName(finalFile);
        const added = await addDownload({
          id,
          url: item.downloadUrl,
          fileName: finalFile,
          category,
          dateAdded: new Date().toISOString(),
          connections: Number(connections),
          speedLimit: speedLimitEnabled ? `${speedLimit}K` : undefined,
          username: useAuth ? username.trim() : undefined,
          password: useAuth ? password.trim() : undefined,
          headers: headers.trim() || undefined,
          checksum: checksumEnabled && checksumValue.trim()
            ? `${checksumAlgo}=${checksumValue.trim()}`
            : undefined,
          cookies: cookies.trim() || undefined,
          mirrors: mirrors.trim() || undefined,
          destination: useSharedDestination
            ? finalLocation
            : destinationOverrides[itemIndex],
          isMedia: item.isMedia,
          mediaFormatSelector: formatSelector,
          size: item.size || (item.sizeBytes ? formatBytes(item.sizeBytes) : undefined)
        }, action);
        if (!added) {
          throw new Error('Backend rejected download start.');
        }
        addedCount += 1;
        } catch (e) {
          console.error("Invalid URL or failed to add:", e);
          failures.push(`${item.file}: ${e instanceof Error ? e.message : String(e)}`);
        }
      }
      toggleAddModal(false);
      if (failures.length > 0) {
        addToast({
          message: `${addedCount} added, ${failures.length} failed. ${failures[0]}`,
          variant: 'error',
          isActionable: true
        });
      } else if (addedCount > 0) {
        addToast({
          message: `${addedCount} download${addedCount === 1 ? '' : 's'} added`,
          variant: 'success'
        });
      }
  };

  const SummaryBox = ({ title, value, icon: Icon, color }: {
    title: string;
    value: string | number;
    icon: LucideIcon;
    color: string;
  }) => (
    <div className="add-download-summary-card flex flex-col">
      <div className="flex items-center gap-1.5 text-text-muted mb-1">
        <Icon size={12} className={color} />
        <span className="text-[10px] font-semibold uppercase tracking-wider">{title}</span>
      </div>
      <span className="text-sm font-semibold text-text-primary truncate">{value}</span>
    </div>
  );

  const selectMediaFormat = (index: number) => {
    if (selectedItemIndex === null) return;
    const selectedItem = parsedItems[selectedItemIndex];
    const format = selectedItem?.formats?.[index];
    if (!selectedItem || !format) return;

    const baseName = selectedItem.file.substring(0, selectedItem.file.lastIndexOf('.')) || selectedItem.file;
    setParsedItems(items => items.map((item, itemIndex) =>
      itemIndex === selectedItemIndex
        ? {
            ...item,
            selectedFormat: index,
            size: format.bytes ? format.detail : undefined,
            sizeBytes: format.bytes || undefined,
            file: canonicalizeDownloadFileName(`${baseName}.${format.ext}`)
          }
        : item
    ));
  };

  const requiredBytes = parsedItems.reduce((acc, item) => acc + (item.sizeBytes || 0), 0);
  const hasApproximateSize = parsedItems.some(item =>
    item.formats?.[item.selectedFormat ?? -1]?.isApproximate
  );
  const requiredStr = requiredBytes > 0
    ? `${hasApproximateSize ? '~' : ''}${requiredBytes < 1024 * 1024 ? `${(requiredBytes / 1024).toFixed(1)} KB`
       : requiredBytes < 1024 * 1024 * 1024 ? `${(requiredBytes / 1024 / 1024).toFixed(1)} MB`
       : `${(requiredBytes / 1024 / 1024 / 1024).toFixed(2)} GB`}`
    : 'Unknown';
  const canSubmit = canSubmitMetadataRows(parsedItems);
  const failedMetadataCount = parsedItems.filter(item => item.status === 'metadata-error').length;

  return (
    <>
      {showingDuplicates && (
        <DuplicateResolutionModal 
          conflicts={conflicts} 
          onConfirm={(resolutions) => {
            setShowingDuplicates(false);
            setIsSubmitting(true);
            void executeAddDownloads(
              pendingAction,
              resolvedLocation,
              pendingUseSharedDestination,
              resolutions,
              pendingDestinationOverrides
            )
              .catch(error => {
                addToast({
                  message: `Could not resolve duplicate downloads: ${String(error)}`,
                  variant: 'error',
                  isActionable: true
                });
              })
              .finally(() => setIsSubmitting(false));
          }} 
          onCancel={() => setShowingDuplicates(false)} 
        />
      )}
    <div className="app-modal-backdrop fixed inset-0 z-50 flex items-center justify-center">
      <div className="app-modal add-download-modal w-[900px] h-[650px] flex flex-col overflow-hidden text-sm">

        {/* Main Content Split */}
        <div className="flex flex-1 overflow-hidden">

          {/* Left Column: URLs and Preview */}
          <div className="add-download-left w-[55%] border-r border-border-modal flex flex-col">
            <div className="add-download-pane p-5 flex-1 flex flex-col gap-5">

              <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <div className="add-download-section-title flex items-center gap-2">
                    <Link size={16} className="text-blue-500" />
                    Download Links
                  </div>
                </div>
                <textarea
                  className="add-download-control add-download-links-input w-full h-32 p-3 text-[13px] resize-none"
                  placeholder={"Paste HTTP, HTTPS, FTP, or SFTP URLs here...\n\nFor media downloads, paste links from Youtube, X, TikTok, Instagram, Reddit, etc."}
                  value={urls}
                  onChange={(e) => setUrls(e.target.value)}
                />
                <div className="flex justify-between items-center px-1">
                  <span className="text-[11px] text-text-muted font-medium">
                    {parsedItems.filter(item => item.status === 'ready').length} ready, {failedMetadataCount} fallback
                  </span>
                    <button
                      type="button"
                      onClick={() => setParsedItems(refreshFailedMetadataRows)}
                      disabled={failedMetadataCount === 0}
                      className="add-download-link-button flex items-center gap-1.5 text-[11px] font-medium"
                    >
                      <RefreshCw size={12} /> Refresh Metadata
                    </button>
                </div>
              </div>

              <div className="grid grid-cols-4 gap-3">
                <SummaryBox title="Files" value={parsedItems.length} icon={FileText} color="text-blue-500" />
                <SummaryBox title="Required" value={requiredStr} icon={Database} color="text-orange-500" />
                <SummaryBox title="Free" value={freeSpace} icon={HardDrive} color="text-green-500" />
                <SummaryBox title="Unknown" value={parsedItems.filter(i => !i.sizeBytes).length} icon={FileText} color="text-purple-500" />
              </div>

              <div className="flex flex-col gap-2 flex-1 overflow-hidden">
                <div className="add-download-section-title flex items-center gap-2">
                  <ArrowRight size={16} className="text-blue-500" />
                  Preview
                </div>
                <div className="add-download-preview flex-1 overflow-hidden flex flex-col">
                  <div className="add-download-preview-header px-3 py-2 flex text-[11px] font-semibold text-text-muted uppercase tracking-wider">
                    <div className="flex-[2]">File</div>
                    <div className="flex-1">Size</div>
                    <div className="flex-[1.5]">Status</div>
                  </div>
                  <div className="flex-1 overflow-y-auto p-2 space-y-1" role="listbox" aria-label="Download preview">
                    {parsedItems.length === 0 ? (
                      <div className="add-download-empty h-full flex items-center justify-center text-text-muted text-xs">
                        No links added yet.
                      </div>
                    ) : (
                      parsedItems.map((item, i) => (
                        <div
                          key={item.id}
                          onClick={() => setSelectedItemIndex(i)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter' || event.key === ' ') {
                              event.preventDefault();
                              setSelectedItemIndex(i);
                            }
                          }}
                          role="option"
                          aria-selected={selectedItemIndex === i}
                          tabIndex={0}
                          className={`add-download-preview-row flex flex-col text-xs px-2 py-2 cursor-pointer rounded-md group ${
                            selectedItemIndex === i
                              ? 'is-selected border'
                              : 'border border-transparent'
                          }`}
                        >
                          <div className="flex items-center w-full">
                            <div className="flex-[2] text-text-primary font-medium truncate pr-2" title={item.file}>{item.file}</div>
                            <div className={`flex-1 font-mono ${item.status === 'loading' ? 'text-text-muted/50' : 'text-text-muted'}`}>{item.size || 'Unknown'}</div>
                            <div className={`flex-[1.5] font-medium ${item.status === 'metadata-error' || item.status === 'invalid' ? 'text-red-500' : item.status === 'loading' ? 'text-orange-400' : 'text-blue-500'}`}>
                              {item.status === 'loading' ? (
                                <div className="flex items-center gap-1.5">
                                  <RefreshCw size={12} className="animate-spin" /> Fetching...
                                </div>
                              ) : (
                                item.status === 'metadata-error'
                                  ? 'Fallback'
                                  : item.status === 'invalid'
                                    ? 'Invalid'
                                    : 'Ready'
                              )}
                            </div>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              </div>

            </div>
          </div>

          {/* Right Column: Settings */}
          <div className="add-download-settings w-[45%] flex flex-col overflow-y-auto">
            <div className="p-6 space-y-5">

              {/* Media Format (Dynamic) */}
              {selectedItemIndex !== null && parsedItems[selectedItemIndex]?.isMedia && (
                <section className="add-download-section add-download-media-section relative overflow-hidden p-4">
                  <div className="absolute top-0 right-0 p-2 opacity-10">
                    <Video size={48} />
                  </div>
                  <div className="add-download-section-title flex items-center gap-2 mb-3 relative z-10">
                    <Video size={16} className="text-purple-500" /> Media Format
                  </div>

                  {parsedItems[selectedItemIndex].status === 'loading' ? (
                    <div className="flex flex-col items-center justify-center py-6 gap-3 relative z-10">
                      <RefreshCw size={24} className="animate-spin text-purple-500" />
                      <span className="text-xs text-text-muted font-medium animate-pulse">Fetching media streams...</span>
                    </div>
                  ) : parsedItems[selectedItemIndex].formats ? (
                    <div className="space-y-3 relative z-10">
                      <div className="flex flex-col gap-1.5">
                        <label className="text-[10px] uppercase font-bold tracking-wider text-text-muted">Available Streams</label>
                        <div className="flex flex-col gap-1 max-h-48 overflow-y-auto pr-1" role="radiogroup" aria-label="Available media streams">
                          {parsedItems[selectedItemIndex].formats!.map((f, idx) => {
                          const isSelected = parsedItems[selectedItemIndex].selectedFormat === idx;
                          const Icon = f.type === 'Audio' ? Music : Film;
                          return (
                            <div
                              key={idx}
                              onClick={() => selectMediaFormat(idx)}
                              onKeyDown={(event) => {
                                if (event.key === 'Enter' || event.key === ' ') {
                                  event.preventDefault();
                                  selectMediaFormat(idx);
                                }
                              }}
                              role="radio"
                              aria-checked={isSelected}
                              tabIndex={0}
                              className={`add-download-format-row flex items-center justify-between px-3 py-2 cursor-pointer text-xs border ${
                                isSelected ? 'is-selected text-purple-600 dark:text-purple-400 font-semibold' : 'text-text-secondary'
                              }`}
                            >
                              <div className="flex items-center gap-2 min-w-0">
                                <Icon size={14} className={isSelected ? 'text-purple-500' : 'text-text-muted'} />
                                <div className="flex flex-col min-w-0">
                                  <span className="truncate">{f.name}</span>
                                  <span className="text-[10px] font-normal text-text-muted truncate">{f.formatLabel}</span>
                                </div>
                              </div>
                              <span className="font-mono text-[11px] opacity-80 shrink-0">{f.detail}</span>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  </div>
                  ) : (
                    <div className="flex flex-col items-center justify-center py-4 relative z-10">
                      <span className="text-xs text-red-400 font-medium">Metadata unavailable. Default media format will be used.</span>
                    </div>
                  )}
                </section>
              )}

              {/* Save Location */}
              <section className="add-download-section">
                <div className="add-download-section-title flex items-center gap-2 mb-3">
                  <FolderPlus size={16} className="text-blue-500" /> Save Location
                </div>
                <div className="flex gap-2">
                  <input
                    type="text"
                    readOnly
                    value={saveLocation}
                    className="add-download-control flex-1 px-3 py-1.5 text-xs text-text-muted font-mono"
                    aria-label="Save location"
                  />
                  <button
                    onClick={handleBrowse}
                    className="add-download-button add-download-button-secondary px-3 text-xs font-medium"
                  >
                    Browse
                  </button>
                </div>
                {parsedItems.length > 1 && !isSaveLocationManual && (
                  <p className="mt-2 text-[11px] text-text-muted">
                    Files will be organized into category folders.
                  </p>
                )}
                {isSaveLocationManual && (
                  <p className="mt-2 text-[11px] text-text-muted">
                    All selected downloads will use this folder.
                  </p>
                )}
              </section>

              {/* Transfer Settings */}
              <section className="add-download-section">
                <div className="add-download-section-title flex items-center gap-2 mb-3">
                  <Settings size={16} className="text-blue-500" /> Transfer Settings
                </div>
                  <div className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                      <label className="text-xs text-text-secondary font-medium">Connections per File</label>
                    <div className="flex items-center gap-2">
                      <input type="range" min="1" max="16" value={connections} onChange={e=>setConnections(Number(e.target.value))} className="add-download-range w-24 accent-blue-500 cursor-pointer" aria-label="Connections per file" />
                      <span className="add-download-value text-xs text-text-primary font-mono w-6 text-center">{connections}</span>
                    </div>
                  </div>

                  <div className="flex items-center justify-between">
                    <label className="flex items-center gap-2 text-xs text-text-secondary font-medium cursor-pointer">
                      <input type="checkbox" checked={speedLimitEnabled} onChange={e=>setSpeedLimitEnabled(e.target.checked)} className="add-download-checkbox" />
                      Limit speed per file
                    </label>
                    {speedLimitEnabled && (
                      <div className="flex items-center gap-1.5">
                        <input type="number" value={speedLimit} onChange={e=>setSpeedLimit(e.target.value)} className="add-download-control w-16 px-2 py-1 text-xs font-mono" aria-label="Speed limit per file" />
                        <span className="text-[10px] text-text-muted">KiB/s</span>
                      </div>
                    )}
                  </div>
                </div>
              </section>

              {/* Authorization */}
              <section className="add-download-section">
                <div className="add-download-section-title flex items-center gap-2 mb-3">
                  <Shield size={16} className="text-blue-500" /> Authorization
                </div>
                <label className="flex items-center gap-2 text-xs text-text-secondary font-medium cursor-pointer mb-3">
                  <input type="checkbox" checked={useAuth} onChange={e=>setUseAuth(e.target.checked)} className="add-download-checkbox" />
                  Use authorization
                </label>

                {useAuth && (
                  <div className="space-y-2.5 pl-5 border-l-2 border-border-modal/50">
                    <input type="text" value={username} onChange={e=>setUsername(e.target.value)} placeholder="Username" className="add-download-control w-full px-3 py-1.5 text-xs" />
                    <input type="password" value={password} onChange={e=>setPassword(e.target.value)} placeholder="Password" className="add-download-control w-full px-3 py-1.5 text-xs" />
                  </div>
                )}
              </section>

              {/* Advanced */}
              <section className="add-download-section add-download-advanced">
                <button
                  onClick={() => setAdvancedExpanded(!advancedExpanded)}
                  className="add-download-advanced-toggle flex items-center gap-2 text-sm font-semibold text-text-primary w-full"
                  aria-expanded={advancedExpanded}
                >
                  {advancedExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                  Advanced Transfer
                </button>

                {advancedExpanded && (
                  <div className="mt-4 space-y-4 pl-6">
                    <label className="flex items-center gap-2 text-xs text-text-secondary font-medium cursor-pointer">
                      <input type="checkbox" checked={checksumEnabled} onChange={e=>setChecksumEnabled(e.target.checked)} className="add-download-checkbox" />
                      Verify Checksum
                    </label>

                    {checksumEnabled && (
                      <div className="flex gap-2">
                        <select value={checksumAlgo} onChange={e=>setChecksumAlgo(e.target.value)} className="add-download-control add-download-select w-24 px-2 text-xs" aria-label="Checksum algorithm">
                          <option>MD5</option><option>SHA-1</option><option>SHA-256</option>
                        </select>
                        <input type="text" value={checksumValue} onChange={e=>setChecksumValue(e.target.value)} placeholder="Expected digest" className="add-download-control flex-1 px-3 py-1.5 text-xs font-mono" />
                      </div>
                    )}

                    <div>
                      <label className="block text-[10px] uppercase font-bold tracking-wider text-text-muted mb-1">Headers</label>
                      <textarea value={headers} onChange={e=>setHeaders(e.target.value)} className="add-download-control w-full h-12 px-3 py-1.5 text-xs font-mono resize-none" aria-label="Request headers" />
                    </div>
                    <div>
                      <label className="block text-[10px] uppercase font-bold tracking-wider text-text-muted mb-1">Cookies</label>
                      <input type="text" value={cookies} onChange={e=>setCookies(e.target.value)} placeholder="name=value; other=value" className="add-download-control w-full px-3 py-1.5 text-xs font-mono" aria-label="Cookies" />
                    </div>
                    <div>
                      <label className="block text-[10px] uppercase font-bold tracking-wider text-text-muted mb-1">Mirrors</label>
                      <textarea value={mirrors} onChange={e=>setMirrors(e.target.value)} className="add-download-control w-full h-12 px-3 py-1.5 text-xs font-mono resize-none" aria-label="Mirrors" />
                    </div>
                  </div>
                )}
              </section>

            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="add-download-footer p-4 flex items-center shrink-0">
          <div className="text-[11px] text-text-muted font-medium flex-1">
            {metadataSummaryMessage(parsedItems)}
          </div>
          <div className="flex gap-2.5">
            <button onClick={() => toggleAddModal(false)} className="add-download-button add-download-button-cancel px-4 text-xs">
              Cancel
            </button>
            <div ref={actionMenuRef} className="relative flex gap-2.5">
              <button
                onClick={() => handleAction({ type: 'start-now' })}
                disabled={!canSubmit || isSubmitting}
                className="add-download-button add-download-button-primary px-5 text-xs"
              >
                <Play size={12} fill="currentColor" /> Start Downloads
              </button>
              <div className="relative">
                <button
                  type="button"
                  onClick={() => setIsQueueMenuOpen(open => !open)}
                  disabled={!canSubmit || isSubmitting}
                  className="add-download-button add-download-button-secondary px-4 text-xs"
                  aria-label="Add to queue"
                  aria-haspopup="menu"
                  aria-expanded={isQueueMenuOpen}
                >
                  Add to queue <ChevronDown size={14} className="ml-1" />
                </button>
                {isQueueMenuOpen && (
                  <div
                    role="menu"
                    className="app-modal absolute bottom-full right-0 z-[70] mb-2 min-w-[200px] overflow-visible py-1.5 text-xs"
                  >
                    {queues.map(queue => (
                      <button
                        key={queue.id}
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setIsQueueMenuOpen(false);
                          void handleAction({ type: 'add-to-queue', queueId: queue.id });
                        }}
                        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-item-hover"
                      >
                        <span className="truncate">{queue.name}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>
    </>
  );
};
