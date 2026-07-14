const FIRELINK_RELEASES_PATH = '/nimbold/Firelink/releases';
const FIRELINK_RELEASE_TAG_PATTERN = new RegExp(
  `^${FIRELINK_RELEASES_PATH}/tag/[A-Za-z0-9._-]+$`
);

export const isTrustedFirelinkReleaseUrl = (value: string) => {
  try {
    const parsed = new URL(value);
    if (
      parsed.protocol !== 'https:'
      || parsed.hostname !== 'github.com'
      || parsed.port
      || parsed.username
      || parsed.password
      || parsed.search
      || parsed.hash
    ) {
      return false;
    }

    const pathname = decodeURIComponent(parsed.pathname);
    return pathname === FIRELINK_RELEASES_PATH
      || FIRELINK_RELEASE_TAG_PATTERN.test(pathname);
  } catch {
    return false;
  }
};
