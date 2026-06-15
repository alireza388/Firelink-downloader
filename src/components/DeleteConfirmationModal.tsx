import React from 'react';
import { useDownloadStore } from '../store/useDownloadStore';
import { AlertTriangle } from 'lucide-react';

export const DeleteConfirmationModal: React.FC = () => {
  const { deleteModalState, closeDeleteModal, removeDownload } = useDownloadStore();

  if (!deleteModalState.isOpen) return null;

  const handleCancel = () => {
    closeDeleteModal();
  };

  const handleRemoveFromList = () => {
    if (deleteModalState.downloadId) {
      removeDownload(deleteModalState.downloadId, false);
    }
    closeDeleteModal();
  };

  const handleDeleteFile = () => {
    if (deleteModalState.downloadId) {
      removeDownload(deleteModalState.downloadId, true);
    }
    closeDeleteModal();
  };

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 animate-fade-in">
      <div 
        className="bg-bg-modal rounded-xl w-full max-w-md overflow-hidden flex flex-col shadow-2xl border border-border-modal scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-5 py-4 border-b border-border-modal flex items-center gap-3">
          <div className="p-2 bg-red-500/10 rounded-full flex items-center justify-center">
            <AlertTriangle size={20} className="text-red-400" />
          </div>
          <h2 className="text-lg font-semibold text-text-primary m-0">
            Remove Download
          </h2>
        </div>

        {/* Body */}
        <div className="px-5 py-6 flex-1 text-sm text-text-secondary leading-relaxed">
          Are you sure you want to remove this item from the list? You can also choose to delete the underlying file from your hard drive.
        </div>

        {/* Footer */}
        <div className="px-5 py-4 border-t border-border-modal flex justify-end gap-3 bg-bg-modal-accent">
          <button
            onClick={handleCancel}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors text-text-secondary hover:bg-item-hover hover:text-text-primary"
          >
            Cancel
          </button>
          <button
            onClick={handleRemoveFromList}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-border-modal hover:bg-border-modal/80 text-text-primary"
          >
            Remove
          </button>
          <button
            onClick={handleDeleteFile}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-red-500/20 text-red-400 hover:bg-red-500/30"
          >
            Delete file
          </button>
        </div>
      </div>
    </div>
  );
};
