import React, { useState } from 'react';
import { useSettingsStore } from '../store/useSettingsStore';
import { invokeCommand as invoke } from '../ipc';
import { KeyRound, ShieldAlert } from 'lucide-react';

export const KeychainPermissionModal: React.FC = () => {
  const showKeychainModal = useSettingsStore(state => state.showKeychainModal);
  const setShowKeychainModal = useSettingsStore(state => state.setShowKeychainModal);
  const [isGranting, setIsGranting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!showKeychainModal) {
    return null;
  }

  const handleGrant = async () => {
    setIsGranting(true);
    setError(null);
    try {
      const result = await invoke('grant_keychain_access');
      if (result.persistent) {
        // Set all keychain-related state directly from the grant result
        // instead of calling hydratePairingToken() again, which would
        // re-read the DB before Zustand's persist middleware has written
        // keychainAccessGranted and could flip persistent back to false.
        useSettingsStore.setState({
          keychainAccessGranted: true,
          extensionPairingToken: result.token,
          isPairingTokenPersistent: true
        });
        setShowKeychainModal(false);
      } else {
        setError(result.error || 'Failed to grant keychain access.');
      }
    } catch (e: any) {
      setError(e.toString());
    } finally {
      setIsGranting(false);
    }
  };

  const handleLater = () => {
    setShowKeychainModal(false);
  };

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 animate-fade-in">
      <div
        className="bg-bg-modal rounded-xl w-full max-w-md overflow-hidden flex flex-col shadow-2xl border border-border-modal scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-4 border-b border-border-modal flex items-center gap-3">
          <div className="p-2 bg-blue-500/10 rounded-full flex items-center justify-center">
            <KeyRound size={20} className="text-blue-500" />
          </div>
        <h2 className="text-lg font-semibold text-text-primary m-0">Credential Storage Access Needed</h2>
        </div>
        
        <div className="px-5 py-6 flex-1 text-sm text-text-secondary leading-relaxed space-y-4">
          <p>
            Firelink uses the browser extension to seamlessly capture downloads.
            To securely store the pairing token that connects the app and the extension,
            we need access to this system's credential store.
          </p>
          <p>
            <strong>Note:</strong> Firelink only requests access to its own dedicated credential entry. It cannot and will not access other saved passwords or credential items on your system.
          </p>
          
          {error && (
            <div className="flex items-start gap-2 text-red-400 bg-red-400/10 p-3 rounded-lg border border-red-400/20 text-xs">
              <ShieldAlert size={14} className="mt-0.5 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}
          
          <div className="bg-bg-modal-accent p-3 rounded-lg border border-border-modal text-xs">
            <strong>Hint:</strong> If you select Later, the extension will only work for this session. 
            You can grant access anytime from <strong>Settings &gt; Integrations</strong>.
          </div>
        </div>
        
        <div className="px-5 py-4 border-t border-border-modal flex justify-end gap-3 bg-bg-modal-accent">
          <button
            onClick={handleLater}
            disabled={isGranting}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors text-text-secondary hover:bg-item-hover hover:text-text-primary disabled:opacity-50"
          >
            Later
          </button>
          <button
            onClick={handleGrant}
            disabled={isGranting}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors bg-accent text-white hover:bg-accent/90 disabled:opacity-50"
          >
            {isGranting ? 'Granting...' : 'Grant Access'}
          </button>
        </div>
      </div>
    </div>
  );
};
