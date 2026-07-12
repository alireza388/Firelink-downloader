import assert from 'node:assert/strict';
import test from 'node:test';

import { parseDebianPackagePath } from './verify-linux-packages.js';

test('parses current dpkg-deb listings without a ./ prefix', () => {
  assert.equal(
    parseDebianPackagePath('drwxr-xr-x 0/0               0 2026-07-12 07:24 usr/share/'),
    'usr/share/'
  );
});

test('parses legacy dpkg-deb listings with a ./ prefix', () => {
  assert.equal(
    parseDebianPackagePath('-rwxr-xr-x root/root     123 2026-07-12 07:24 ./usr/bin/firelink'),
    'usr/bin/firelink'
  );
});

test('rejects malformed dpkg-deb listing lines', () => {
  assert.throws(
    () => parseDebianPackagePath('not a dpkg-deb listing'),
    /Could not parse a Debian package path/
  );
});
