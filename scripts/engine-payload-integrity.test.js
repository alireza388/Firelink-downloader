import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { collectRegularFiles, treeDigest } from './engine-payload-integrity.js';

test('collectRegularFiles returns regular files and honors ignored names', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'firelink-engine-payload-'));
  try {
    fs.mkdirSync(path.join(root, 'nested'));
    fs.writeFileSync(path.join(root, 'engine'), 'binary');
    fs.writeFileSync(path.join(root, 'nested', 'runtime.dat'), 'runtime');
    fs.writeFileSync(path.join(root, 'payload-manifest.json'), '{}');

    const files = collectRegularFiles(root, {
      ignoredNames: ['payload-manifest.json'],
    }).map(file => path.relative(root, file).split(path.sep).join('/'));

    assert.deepEqual(files, ['engine', 'nested/runtime.dat']);
    const digest = treeDigest(path.join(root, 'nested'));
    assert.equal(digest.files, 1);
    assert.match(digest.sha256, /^[a-f0-9]{64}$/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('collectRegularFiles rejects symlinks in engine payloads', { skip: process.platform === 'win32' }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'firelink-engine-payload-'));
  try {
    fs.writeFileSync(path.join(root, 'target'), 'target');
    fs.symlinkSync('target', path.join(root, 'link'));

    assert.throws(
      () => collectRegularFiles(root),
      /Unsupported symlink in engine payload: link/
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
