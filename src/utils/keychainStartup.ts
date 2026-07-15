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

// The semantic app version can remain unchanged across release-candidate and
// packaging rebuilds. Bump this contract version whenever a build can require
// a fresh credential-store authorization, so an updated binary cannot skip
// Firelink's explanation and invoke the OS prompt directly.
const KEYCHAIN_CONSENT_POLICY_VERSION = '2';

export const getKeychainConsentVersion = (appVersion: string): string => {
  const normalizedVersion = appVersion.trim();
  return normalizedVersion
    ? `${normalizedVersion}|keychain-policy-${KEYCHAIN_CONSENT_POLICY_VERSION}`
    : '';
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
