import React, { useState, useEffect } from 'react';
import { useDownloadStore } from '../store/useDownloadStore';
import { useSettingsStore } from '../store/useSettingsStore';
import { categoryForFileName } from '../utils/downloads';

const formatBytes = (bytes: number) => {
  if (bytes === 0) return 'Unknown size';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

export const QualityModal = React.memo(() => {
  const { activeMetadata, activeMetadataUrl, isParsing, parsingError, clearMetadata, addDownload } = useDownloadStore();
  const [selectedFormatId, setSelectedFormatId] = useState<string>('');

  useEffect(() => {
    if (activeMetadata && activeMetadata.formats.length > 0 && !selectedFormatId) {
      setSelectedFormatId(activeMetadata.formats[0].format_id);
    }
  }, [activeMetadata]);

  if (!isParsing && !activeMetadata && !parsingError) return null;

  const handleConfirm = () => {
    if (!activeMetadata || !activeMetadataUrl || !selectedFormatId) return;

    const format = activeMetadata.formats.find(f => f.format_id === selectedFormatId);
    if (!format) return;

    const settings = useSettingsStore.getState();
    const id = crypto.randomUUID();
    const filename = `${activeMetadata.title}.${format.ext}`.replace(/[\/\\?%*:|"<>]/g, '-');
    
    const category = categoryForFileName(filename);
    const destination = (settings.downloadDirectories && settings.downloadDirectories[category]) || settings.defaultDownloadPath || '~/Downloads';

    const downloadItem = {
      id,
      url: activeMetadataUrl,
      fileName: filename,
      destination,
      status: 'queued' as const,
      fraction: 0,
      size: format.filesize ? formatBytes(format.filesize) : 'Unknown',
      speed: '-',
      eta: '-',
      dateAdded: new Date().toISOString(),
      queueId: '00000000-0000-0000-0000-000000000001',
      _dispatched: false,
      isMedia: true,
      selectedFormatId: format.format_id
    };

    addDownload(downloadItem as any);
    clearMetadata();
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-[#1a1b1e] rounded-xl p-6 max-w-lg w-full shadow-2xl border border-gray-200 dark:border-gray-800">
        <h2 className="text-xl font-bold mb-4 text-gray-900 dark:text-gray-100">Media Quality Selection</h2>
        
        {isParsing && (
          <div className="py-8 flex flex-col items-center justify-center text-gray-500 dark:text-gray-400">
            <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500 mb-4"></div>
            <p>Parsing media metadata...</p>
          </div>
        )}
        
        {parsingError && (
          <div className="py-4 text-red-500 bg-red-50 dark:bg-red-900/20 p-4 rounded-lg">
            <p className="font-semibold mb-1">Parsing Failed</p>
            <p className="text-sm">{parsingError}</p>
            <div className="mt-4 flex justify-end">
              <button onClick={clearMetadata} className="px-4 py-2 border border-red-200 dark:border-red-800 rounded hover:bg-red-100 dark:hover:bg-red-900/40">Close</button>
            </div>
          </div>
        )}
        
        {activeMetadata && (
          <>
            <div className="mb-6">
              <p className="font-semibold text-gray-900 dark:text-gray-100 mb-1 truncate">{activeMetadata.title}</p>
              {activeMetadata.duration && <p className="text-sm text-gray-500">Duration: {Math.floor(activeMetadata.duration / 60)}:{String(activeMetadata.duration % 60).padStart(2, '0')}</p>}
            </div>
            
            <div className="mb-6">
              <label className="block text-sm font-medium mb-2 text-gray-700 dark:text-gray-300">Select Format</label>
              <select 
                className="w-full border rounded-lg p-3 bg-gray-50 dark:bg-[#25262b] border-gray-200 dark:border-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-blue-500 outline-none transition-shadow"
                value={selectedFormatId}
                onChange={(e) => setSelectedFormatId(e.target.value)}
              >
                {activeMetadata.formats.map(f => (
                  <option key={f.format_id} value={f.format_id}>
                    {f.resolution || 'Best'} • {f.format_label || f.ext.toUpperCase()} • {f.filesize ? formatBytes(f.filesize) : 'Unknown size'}
                  </option>
                ))}
              </select>
            </div>

            <div className="flex justify-end gap-3 mt-8">
              <button 
                onClick={clearMetadata} 
                className="px-5 py-2.5 text-sm font-medium border border-gray-200 dark:border-gray-700 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
              >
                Cancel
              </button>
              <button 
                onClick={handleConfirm} 
                disabled={!selectedFormatId}
                className="px-5 py-2.5 text-sm font-medium bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                Confirm Download
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
});
