export type KeychainStartupState = {
  portable: boolean;
  appVersion: string;
  approvedVersion: string;
  accessGranted: boolean;
  promptDismissed: boolean;
};

export type KeychainStartupDecision = {
  deferKeychainHydration: boolean;
  showKeychainPrompt: boolean;
};

export const getKeychainStartupDecision = ({
  portable,
  appVersion,
  approvedVersion,
  accessGranted,
  promptDismissed
}: KeychainStartupState): KeychainStartupDecision => {
  if (portable) {
    return {
      deferKeychainHydration: false,
      showKeychainPrompt: false
    };
  }

  const versionChanged = Boolean(appVersion) && approvedVersion !== appVersion;
  return {
    deferKeychainHydration: !accessGranted || versionChanged,
    showKeychainPrompt: versionChanged || (!accessGranted && !promptDismissed)
  };
};
