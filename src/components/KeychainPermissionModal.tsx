import React, { useState } from 'react';
import { useSettingsStore } from '../store/useSettingsStore';
import { invokeCommand as invoke } from '../ipc';
import { KeyRound, ShieldAlert } from 'lucide-react';
import { usePlatformInfo } from '../utils/platform';

export const KeychainPermissionModal: React.FC = () => {
  const showKeychainModal = useSettingsStore(state => state.showKeychainModal);
  const setShowKeychainModal = useSettingsStore(state => state.setShowKeychainModal);
  const platform = usePlatformInfo();
  const [isGranting, setIsGranting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!showKeychainModal) {
    return null;
  }

  const isMac = platform.os === 'macos';
  const storeName =
    platform.portable
      ? 'the portable Firelink data folder'
      : platform.os === 'windows'
      ? 'Windows Credential Manager'
      : platform.os === 'linux'
        ? 'your Linux credential store'
        : platform.os === 'macos'
          ? 'macOS Keychain'
          : "this system's credential store";
  const grantLabel = platform.portable
    ? 'Enable Portable Pairing'
    : isMac
      ? 'Grant Access'
      : 'Enable Secure Storage';

  const handleGrant = async () => {
    setIsGranting(true);
    setError(null);

    try {
      const result = await invoke('grant_keychain_access');
      if (result.persistent) {
        // Keep state in sync with the grant result instead of rehydrating
        // before Zustand has persisted keychainAccessGranted.
        useSettingsStore.setState({
          keychainAccessGranted: true,
          extensionPairingToken: result.token,
          isPairingTokenPersistent: true
        });
        setShowKeychainModal(false);
      } else {
        setError(result.error || `${storeName} is unavailable.`);
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
    <div className="app-modal-backdrop fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        className="window-safe-modal bg-bg-modal rounded-xl w-full max-w-md overflow-hidden flex flex-col shadow-2xl border border-border-modal scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-4 border-b border-border-modal flex items-center gap-3">
          <div className="p-2 bg-blue-500/10 rounded-full items-center justify-center">
            <KeyRound size={20} className="text-blue-500" />
          </div>
          <h2 className="text-lg font-semibold text-text-primary m-0">Credential Storage Access Needed</h2>
        </div>

        <div className="px-5 py-6 flex-1 min-h-0 overflow-y-auto text-sm text-text-secondary leading-relaxed space-y-4">
          <p>
            Firelink uses the browser extension to capture downloads. To keep the extension paired after restarts,
            Firelink stores its pairing token in {storeName}.
          </p>

          <p>
            {platform.portable
              ? 'The pairing token is portable with this folder. Treat the folder as sensitive and do not share it.'
              : isMac
              ? 'macOS may show a Keychain prompt after you grant access.'
              : 'This usually completes silently. If the credential service is unavailable, Firelink will show the error here and the extension will stay paired for this session only.'}
          </p>

          <p>
            <strong>Note:</strong>{' '}
            {platform.portable
              ? 'Portable mode stores only the pairing token in this folder. It does not copy site passwords or browser credentials.'
              : 'Firelink only writes its own dedicated credential entry. It cannot access other saved passwords or credential items on your system.'}
          </p>

          {error && (
            <div className="flex items-start gap-2 text-red-400 bg-red-400/10 p-3 rounded-lg border border-red-400/20 text-xs">
              <ShieldAlert size={14} className="mt-0.5 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}

          <div className="bg-bg-modal-accent p-3 rounded-lg border border-border-modal text-xs">
            <strong>Hint:</strong>{' '}
            {platform.portable
              ? 'The portable pairing token is already stored with this folder; you can enable it here or select Later.'
              : 'If you select Later, the extension will only work for this session.'}
            You can enable storage anytime from <strong>Settings &gt; Integrations</strong>.
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
            {isGranting ? 'Enabling...' : grantLabel}
          </button>
        </div>
      </div>
    </div>
  );
};
