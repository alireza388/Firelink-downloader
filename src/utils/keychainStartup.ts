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

  const versionKnown = Boolean(appVersion.trim());
  const versionChanged = versionKnown && approvedVersion !== appVersion;
  const mustDeferAccess = !accessGranted || !versionKnown || versionChanged;
  return {
    deferKeychainHydration: mustDeferAccess,
    showKeychainPrompt: versionChanged || (!accessGranted && !promptDismissed)
  };
};
