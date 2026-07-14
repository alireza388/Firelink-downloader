import { describe, expect, it } from 'vitest';
import { isTrustedFirelinkReleaseUrl } from './releaseUrls';

describe('Firelink release URLs', () => {
  it('accepts the repository release page and exact release tags', () => {
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases')).toBe(true);
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases/tag/v1.0.5')).toBe(true);
  });

  it('rejects lookalike paths and URLs with authority or path tricks', () => {
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases-evil')).toBe(false);
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases%2F..%2Fother')).toBe(false);
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases/tag/v1.0.5%5C..%5Cuser%5Crepo')).toBe(false);
    expect(isTrustedFirelinkReleaseUrl('https://github.com.evil.example/nimbold/Firelink/releases')).toBe(false);
    expect(isTrustedFirelinkReleaseUrl('https://github.com/nimbold/Firelink/releases/tag/v1.0.5?redirect=evil')).toBe(false);
  });
});
