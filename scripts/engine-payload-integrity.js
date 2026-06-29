import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

export function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

export function collectRegularFiles(root, options = {}) {
  const ignoredNames = new Set(options.ignoredNames || []);
  const files = [];

  const walk = directory => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      const relative = path.relative(root, file).split(path.sep).join('/');
      if (ignoredNames.has(entry.name)) {
        continue;
      }
      if (entry.isSymbolicLink()) {
        throw new Error(`Unsupported symlink in engine payload: ${relative}`);
      }
      if (entry.isDirectory()) {
        walk(file);
      } else if (entry.isFile()) {
        files.push(file);
      } else {
        throw new Error(`Unsupported filesystem entry in engine payload: ${relative}`);
      }
    }
  };

  walk(root);
  return files.sort((left, right) => left.localeCompare(right));
}

export function treeDigest(root) {
  const files = collectRegularFiles(root);
  const digest = crypto.createHash('sha256');
  for (const file of files) {
    const relative = path.relative(root, file).split(path.sep).join('/');
    digest.update(`${relative}\0${sha256(file)}\n`);
  }
  return { files: files.length, sha256: digest.digest('hex') };
}
