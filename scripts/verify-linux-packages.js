#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { collectRegularFiles, sha256 } from './engine-payload-integrity.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const __filename = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(__dirname, '..');

function argValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function fail(message) {
  console.error(`[FAIL] ${message}`);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, ...options.env },
    stdio: options.stdio ?? 'inherit',
    encoding: options.stdio === 'pipe' ? 'utf8' : undefined,
  });

  if (result.error) {
    fail(`Failed to run ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    if (options.stdio === 'pipe') {
      if (result.stdout) process.stdout.write(result.stdout);
      if (result.stderr) process.stderr.write(result.stderr);
    }
    fail(`${command} exited with status ${result.status}`);
  }
  return result;
}

function findSingle(directory, extension, label) {
  if (!fs.existsSync(directory) || !fs.statSync(directory).isDirectory()) {
    fail(`${label} directory does not exist: ${directory}`);
  }

  const matches = fs.readdirSync(directory)
    .filter(name => name.endsWith(extension))
    .map(name => path.join(directory, name));
  if (matches.length !== 1) {
    fail(`Expected exactly one ${label}, found ${matches.length} in ${directory}`);
  }
  return matches[0];
}

function assertPackageListing(packageFile, packageType, expectedPath) {
  const result = packageType === 'deb'
    ? run('dpkg-deb', ['--contents', packageFile], { stdio: 'pipe' })
    : run('rpm', ['-qpl', packageFile], { stdio: 'pipe' });
  const listing = result.stdout ?? '';
  assertSafePackageListing(listing, packageType);
  if (!listing.includes(expectedPath)) {
    fail(`${packageType} package is missing ${expectedPath}`);
  }
  if (!/usr\/share\/applications\/[^/]+\.desktop/.test(listing)) {
    fail(`${packageType} package is missing its desktop entry`);
  }
}

function assertPackageRecommendations(packageFile, packageType) {
  const result = packageType === 'deb'
    ? run('dpkg-deb', ['--field', packageFile, 'Recommends'], { stdio: 'pipe' })
    : run('rpm', ['-qp', '--recommends', packageFile], { stdio: 'pipe' });
  const recommendations = result.stdout ?? '';
  const dependencyNames = packageType === 'deb'
    ? recommendations
      .split(/[,|]/)
      .map(value => value.trim().split(/\s+/, 1)[0]?.split(':', 1)[0])
    : recommendations
      .split('\n')
      .map(value => value.trim().split(/\s+/, 1)[0]);
  for (const dependency of ['desktop-file-utils', 'xdg-utils']) {
    if (!dependencyNames.includes(dependency)) {
      fail(`${packageType} package is missing its ${dependency} recommendation`);
    }
  }
}

export function parseDebianPackagePath(line) {
  const match = line.match(/^\S+\s+\S+\s+\S+\s+\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}\s+(.*)$/);
  if (!match) {
    throw new Error(`Could not parse a Debian package path: ${line}`);
  }
  return match[1].replace(/^\.\//, '');
}

export function isSafePackagePath(packagePath) {
  if (packagePath === '') {
    return true;
  }

  const parts = packagePath.split('/');
  return !parts.includes('..') && (parts[0] === 'usr' || packagePath === 'usr');
}

function assertSafePackageListing(listing, packageType) {
  const lines = listing.split('\n').filter(Boolean);
  const paths = packageType === 'deb'
    ? lines.map(line => {
      try {
        return parseDebianPackagePath(line);
      } catch (error) {
        fail(error.message);
      }
    })
    : lines.map(line => line.replace(/^\/+/, ''));

  for (const packagePath of paths) {
    if (!isSafePackagePath(packagePath)) {
      fail(`${packageType} package contains an unsafe path: ${packagePath}`);
    }
  }
}

function extractDeb(packageFile, destination) {
  fs.mkdirSync(destination, { recursive: true });
  run('dpkg-deb', ['--extract', packageFile, destination]);
}

function extractRpm(packageFile, destination) {
  fs.mkdirSync(destination, { recursive: true });
  run('bsdtar', [
    '--extract',
    '--file', packageFile,
    '--directory', destination,
    '--no-same-owner',
    '--no-same-permissions',
  ]);
}

function readPayloadManifest(root, label) {
  const manifestPath = path.join(root, 'payload-manifest.json');
  if (!fs.existsSync(manifestPath)) {
    fail(`${label} payload manifest is missing`);
  }
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  } catch (error) {
    fail(`${label} payload manifest is invalid: ${error.message}`);
  }
  return manifest;
}

function findPayloadRoot(root, target, label) {
  const expectedBinary = `yt-dlp-${target}`;
  const matches = [];
  const walk = directory => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const candidate = path.join(directory, entry.name);
      if (!entry.isDirectory()) continue;
      if (fs.existsSync(path.join(candidate, expectedBinary)) && fs.existsSync(path.join(candidate, 'payload-manifest.json'))) {
        matches.push(candidate);
      }
      walk(candidate);
    }
  };
  walk(root);
  if (matches.length !== 1) {
    fail(`Expected exactly one ${label} engine payload root, found ${matches.length}`);
  }
  return matches[0];
}

function assertPayloadMatchesSource(sourceRoot, packagedRoot, target, label) {
  const sourceManifest = readPayloadManifest(sourceRoot, 'Provisioned engine');
  if (sourceManifest.target !== target) {
    fail(`Provisioned engine payload target mismatch: expected ${target}, got ${sourceManifest.target}`);
  }

  const packagedManifest = readPayloadManifest(packagedRoot, label);
  if (packagedManifest.target !== target) {
    fail(`${label} payload target mismatch: expected ${target}, got ${packagedManifest.target}`);
  }

  const expectedFiles = Object.keys(sourceManifest.files || {}).sort();
  const packagedFiles = collectRegularFiles(packagedRoot, { ignoredNames: ['payload-manifest.json'] })
    .map(file => path.relative(packagedRoot, file).split(path.sep).join('/'))
    .sort();
  if (JSON.stringify(packagedFiles) !== JSON.stringify(expectedFiles)) {
    fail(`${label} payload files differ from the provisioned engine manifest`);
  }

  for (const relative of expectedFiles) {
    const packagedFile = path.join(packagedRoot, relative);
    if (sha256(packagedFile) !== sourceManifest.files[relative]) {
      fail(`${label} payload checksum mismatch: ${relative}`);
    }
  }
}

function findExecutable(root) {
  const candidates = [
    path.join(root, 'usr', 'bin', 'firelink'),
    path.join(root, 'usr', 'bin', 'Firelink'),
  ];
  const executable = candidates.find(candidate => {
    if (!fs.existsSync(candidate)) return false;
    const stat = fs.lstatSync(candidate);
    return stat.isFile() && !stat.isSymbolicLink();
  });
  if (!executable) {
    fail(`Packaged Firelink executable was not found under ${root}`);
  }
  return executable;
}

function verifyExtractedPackage(packageType, packageFile, target, root) {
  const sourceRoot = path.join(repoRoot, 'src-tauri', 'provisioned-engines', target);
  const packagedRoot = findPayloadRoot(root, target, packageType);
  assertPayloadMatchesSource(sourceRoot, packagedRoot, target, packageType);
  run(process.execPath, [
    path.join(repoRoot, 'scripts', 'verify-binaries.js'),
    '--search-root',
    root,
    '--target',
    target,
  ]);

  const executable = findExecutable(root);
  run('xvfb-run', [
    '-a',
    process.execPath,
    path.join(repoRoot, 'scripts', 'smoke-packaged-app.js'),
    '--executable',
    executable,
  ], { env: { APPDIR: root } });
}

function main() {
  const target = argValue('--target');
  if (!target) fail('Pass --target <Rust target triple>.');
  if (os.platform() !== 'linux') fail('Linux package verification must run on Linux.');

  const bundleRoot = path.join(repoRoot, 'src-tauri', 'target', target, 'release', 'bundle');
  const deb = findSingle(path.join(bundleRoot, 'deb'), '.deb', 'Debian package');
  const rpm = findSingle(path.join(bundleRoot, 'rpm'), '.rpm', 'RPM package');
  const extractionRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'firelink-linux-packages-'));
  const debRoot = path.join(extractionRoot, 'deb');
  const rpmRoot = path.join(extractionRoot, 'rpm');

  try {
    assertPackageListing(deb, 'deb', 'usr/share/metainfo/com.nimbold.firelink.metainfo.xml');
    assertPackageRecommendations(deb, 'deb');
    extractDeb(deb, debRoot);
    verifyExtractedPackage('deb', deb, target, debRoot);

    assertPackageListing(rpm, 'rpm', 'usr/share/metainfo/com.nimbold.firelink.metainfo.xml');
    assertPackageRecommendations(rpm, 'rpm');
    extractRpm(rpm, rpmRoot);
    verifyExtractedPackage('rpm', rpm, target, rpmRoot);
    console.log('Linux .deb and .rpm payload and launch verification passed.');
  } finally {
    fs.rmSync(extractionRoot, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === __filename) {
  main();
}
