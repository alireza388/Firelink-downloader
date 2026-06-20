import React, { useState } from 'react';
import { useDownloadStore } from '../store/useDownloadStore';
import { AlertTriangle } from 'lucide-react';

export const DeleteConfirmationModal: React.FC = () => {
  const { deleteModalState, closeDeleteModal, removeDownload } = useDownloadStore();
  const [errorMessage, setErrorMessage] = useState('');
  const [isRemoving, setIsRemoving] = useState(false);

  if (!deleteModalState.isOpen) return null;

  const handleCancel = () => {
    closeDeleteModal();
  };

  const handleRemoveFromList = async () => {
    if (deleteModalState.downloadIds && deleteModalState.downloadIds.length > 0) {
      setIsRemoving(true);
      try {
        await Promise.all(deleteModalState.downloadIds.map(id => removeDownload(id, false)));
      } catch (error) {
        setErrorMessage(`Remove failed: ${String(error)}`);
        setIsRemoving(false);
        return;
      }
    }
    closeDeleteModal();
  };

  const handleDeleteFile = async () => {
    if (deleteModalState.downloadIds && deleteModalState.downloadIds.length > 0) {
      setIsRemoving(true);
      try {
        await Promise.all(deleteModalState.downloadIds.map(id => removeDownload(id, true)));
      } catch (error) {
        setErrorMessage(`Delete failed: ${String(error)}`);
        setIsRemoving(false);
        return;
      }
    }
    closeDeleteModal();
  };

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 animate-fade-in">
      <div
        className="bg-bg-modal rounded-xl w-full max-w-md overflow-hidden flex flex-col shadow-2xl border border-border-modal scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-4 border-b border-border-modal flex items-center gap-3">
          <div className="p-2 bg-red-500/10 rounded-full flex items-center justify-center">
            <AlertTriangle size={20} className="text-red-400" />
          </div>
          <h2 className="text-lg font-semibold text-text-primary m-0">Remove Download</h2>
        </div>

        <div className="px-5 py-6 flex-1 text-sm text-text-secondary leading-relaxed">
          {`Are you sure you want to remove ${deleteModalState.downloadIds?.length && deleteModalState.downloadIds.length > 1 ? 'these ' + deleteModalState.downloadIds.length + ' items' : 'this item'} from the list? You can also choose to delete the underlying file from your hard drive.`}
          {errorMessage && <div className="mt-3 text-xs text-red-400">{errorMessage}</div>}
        </div>

        <div className="px-5 py-4 border-t border-border-modal flex justify-end gap-3 bg-bg-modal-accent">
          <button
            onClick={handleCancel}
            disabled={isRemoving}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors text-text-secondary hover:bg-item-hover hover:text-text-primary disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={handleRemoveFromList}
            disabled={isRemoving}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-border-modal hover:bg-border-modal/80 text-text-primary disabled:opacity-50"
          >
            Remove
          </button>
          <button
            onClick={handleDeleteFile}
            disabled={isRemoving}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-red-500/20 text-red-400 hover:bg-red-500/30 disabled:opacity-50"
          >
            Delete file
          </button>
        </div>
      </div>
    </div>
  );
};
