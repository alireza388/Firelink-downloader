#!/usr/bin/env node
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const sourceLock = JSON.parse(
  fs.readFileSync(path.join(repoRoot, 'engine-sources.lock.json'), 'utf8')
);

function argValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const target = argValue('--target')
  || process.env.FIRELINK_TARGET_TRIPLE
  || process.env.TAURI_ENV_TARGET_TRIPLE;
if (!target) {
  console.error('Pass --target <Rust target triple>.');
  process.exit(1);
}

const targetSources = sourceLock.targets?.[target];
if (!targetSources) {
  console.error(`No source lock exists for ${target}.`);
  process.exit(1);
}

const destination = path.join(repoRoot, 'src-tauri', 'provisioned-engines', target);
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), `firelink-engines-${target}-`));
const isWindows = target.includes('windows');
const executableSuffix = isWindows ? '.exe' : '';

function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

async function download(name, source) {
  const sourcePath = new URL(source.url).pathname;
  const archive = path.join(
    temporary,
    `${name}${sourcePath.endsWith('.tar.xz') ? '.tar.xz' : '.zip'}`
  );
  const response = await fetch(source.url, { redirect: 'follow' });
  if (!response.ok || !response.body) {
    throw new Error(`Failed to download ${name}: HTTP ${response.status}`);
  }
  const output = fs.createWriteStream(archive);
  const reader = response.body.getReader();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!output.write(Buffer.from(value))) {
      await new Promise(resolve => output.once('drain', resolve));
    }
  }
  await new Promise(resolve => output.end(resolve));

  const actual = sha256(archive);
  if (actual !== source.sha256) {
    throw new Error(`Archive checksum mismatch for ${name}. Expected ${source.sha256}, got ${actual}`);
  }
  const extracted = path.join(temporary, `${name}-extracted`);
  fs.mkdirSync(extracted);
  if (archive.endsWith('.zip') && process.platform !== 'win32') {
    execFileSync('unzip', ['-q', archive, '-d', extracted], { stdio: 'inherit' });
  } else {
    execFileSync('tar', ['-xf', archive, '-C', extracted], { stdio: 'inherit' });
  }
  return extracted;
}

function findFile(root, names) {
  const wanted = new Set(names.map(name => name.toLowerCase()));
  const matches = [];
  const walk = directory => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile() && wanted.has(entry.name.toLowerCase())) matches.push(file);
    }
  };
  walk(root);
  if (matches.length !== 1) {
    throw new Error(`Expected one of [${names.join(', ')}] under ${root}, found ${matches.length}`);
  }
  return matches[0];
}

function copyExecutable(source, engine) {
  const output = path.join(destination, `${engine}-${target}${executableSuffix}`);
  fs.copyFileSync(source, output);
  if (!isWindows) fs.chmodSync(output, 0o755);
}

function writePayloadManifest() {
  const files = [];
  const walk = directory => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile() && entry.name !== 'payload-manifest.json') files.push(file);
    }
  };
  walk(destination);
  files.sort((left, right) => left.localeCompare(right));
  const manifest = {
    schemaVersion: 1,
    target,
    generatedFrom: Object.fromEntries(
      Object.entries(targetSources).map(([name, source]) => [
        name,
        {
          version: source.version,
          url: source.url || source.sourceUrl,
          sha256: source.sha256 || source.sourceSha256
        }
      ])
    ),
    files: Object.fromEntries(
      files.map(file => [
        path.relative(destination, file).split(path.sep).join('/'),
        sha256(file)
      ])
    )
  };
  fs.writeFileSync(
    path.join(destination, 'payload-manifest.json'),
    `${JSON.stringify(manifest, null, 2)}\n`
  );
}

try {
  fs.rmSync(destination, { recursive: true, force: true });
  fs.mkdirSync(destination, { recursive: true });

  const ytdlp = await download('yt-dlp', targetSources['yt-dlp']);
  copyExecutable(
    findFile(ytdlp, isWindows ? ['yt-dlp.exe'] : ['yt-dlp_linux']),
    'yt-dlp'
  );
  fs.cpSync(path.join(ytdlp, '_internal'), path.join(destination, '_internal'), {
    recursive: true,
    preserveTimestamps: true
  });

  const deno = await download('deno', targetSources.deno);
  copyExecutable(findFile(deno, isWindows ? ['deno.exe'] : ['deno']), 'deno');

  const ffmpeg = await download('ffmpeg', targetSources.ffmpeg);
  copyExecutable(findFile(ffmpeg, isWindows ? ['ffmpeg.exe'] : ['ffmpeg']), 'ffmpeg');

  const aria2 = await download('aria2c', targetSources.aria2c);
  copyExecutable(findFile(aria2, isWindows ? ['aria2c.exe'] : ['aria2c']), 'aria2c');

  writePayloadManifest();
  console.log(`Provisioned locked engine payload at ${destination}`);
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
