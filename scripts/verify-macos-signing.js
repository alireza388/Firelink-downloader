#!/usr/bin/env node
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';

function argValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const appArg = argValue('--app');
const dmgArg = argValue('--dmg');

if (process.platform !== 'darwin') {
  console.error('macOS signing verification must run on a macOS host.');
  process.exit(1);
}

if (!appArg && !dmgArg) {
  console.error('Pass --app <Firelink.app> and/or --dmg <Firelink.dmg>.');
  process.exit(1);
}

let exitCode = 0;

function fail(message) {
  console.error(`[FAIL] ${message}`);
  exitCode = 1;
}

function ok(message) {
  console.log(`[OK] ${message}`);
}

function note(message) {
  console.log(`[INFO] ${message}`);
}

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
}

function runResult(command, args) {
  return spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

function assertAppPath(appPath, label) {
  if (!fs.existsSync(appPath)) {
    fail(`${label} does not exist at ${appPath}`);
    return false;
  }

  const stat = fs.statSync(appPath);
  if (!stat.isDirectory() || path.extname(appPath) !== '.app') {
    fail(`${label} is not an .app bundle: ${appPath}`);
    return false;
  }

  ok(`${label} exists: ${appPath}`);
  return true;
}

function codesignDetails(targetPath) {
  const result = runResult('codesign', ['-dv', '--verbose=4', targetPath]);
  return {
    ok: result.status === 0,
    output: `${result.stdout || ''}${result.stderr || ''}`,
  };
}

function verifyCodeSignature(targetPath, label, options = {}) {
  const {
    deep = false,
    quietOk = false,
    requireAdhoc = false,
    warnOnVerifyFailure = false,
  } = options;
  const verifyArgs = ['--verify'];
  if (deep) {
    verifyArgs.push('--deep');
  }
  verifyArgs.push('--strict', '--verbose=2', targetPath);

  const details = codesignDetails(targetPath);
  if (!details.ok) {
    fail(`${label}: no readable code signature: ${details.output.trim() || 'codesign -dv failed'}`);
    return 'failed';
  }

  let status = 'verified';
  try {
    run('codesign', verifyArgs);
    if (!quietOk) {
      ok(`${label}: codesign verification passed`);
    }
  } catch (error) {
    const detail = error.stderr?.trim() || error.message;
    if (!warnOnVerifyFailure) {
      fail(`${label}: codesign verification failed: ${detail}`);
      return 'failed';
    }
    note(`${label}: signed, but individual verification warned: ${detail}`);
    status = 'warning';
  }

  if (requireAdhoc && !details.output.includes('Signature=adhoc')) {
    fail(`${label}: expected ad-hoc signature, but codesign did not report Signature=adhoc`);
    return 'failed';
  }
  if (requireAdhoc && !quietOk) {
    ok(`${label}: ad-hoc signature present`);
  }

  return status;
}

function walkFiles(root, visitor) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const entryPath = path.join(root, entry.name);
    if (entry.isSymbolicLink()) {
      continue;
    }
    if (entry.isDirectory()) {
      walkFiles(entryPath, visitor);
      continue;
    }
    if (entry.isFile()) {
      visitor(entryPath);
    }
  }
}

function fileBrief(filePath) {
  try {
    return run('file', ['--brief', filePath], { timeout: 5000 }).trim();
  } catch (error) {
    fail(`file(1) failed for ${filePath}: ${error.stderr?.trim() || error.message}`);
    return '';
  }
}

function verifyMachOObjects(appPath, label) {
  const machOFiles = [];
  walkFiles(appPath, filePath => {
    const description = fileBrief(filePath);
    if (description.includes('Mach-O')) {
      machOFiles.push(filePath);
    }
  });

  if (machOFiles.length === 0) {
    fail(`${label}: no Mach-O files found inside app bundle`);
    return;
  }

  let warningCount = 0;
  let signedCount = 0;
  let verifiedCount = 0;
  for (const filePath of machOFiles) {
    const relative = path.relative(appPath, filePath).split(path.sep).join('/');
    const isPrimaryExecutable = relative === 'Contents/MacOS/firelink';
    const isDirectEngine = /^Contents\/Resources\/engine-dist\/[^/]+\/(?:yt-dlp|aria2c|ffmpeg|deno)-/.test(relative);
    const mayWarn = !isPrimaryExecutable && !isDirectEngine;

    const result = verifyCodeSignature(filePath, `${label} ${relative}`, {
      quietOk: true,
      warnOnVerifyFailure: mayWarn,
    });
    if (result !== 'failed') {
      signedCount += 1;
      if (result === 'verified') {
        verifiedCount += 1;
      } else {
        warningCount += 1;
      }
    }
  }
  ok(`${label}: found signatures on ${signedCount}/${machOFiles.length} Mach-O code object(s)`);
  ok(`${label}: individually verified ${verifiedCount}/${machOFiles.length} Mach-O code object(s)`);
  if (warningCount > 0) {
    note(`${label}: ${warningCount} nested signed framework object(s) produced individual verification warnings; the outer bundle signature remains authoritative.`);
  }
}

function assessGatekeeper(appPath, label) {
  const result = runResult('spctl', ['--assess', '--type', 'execute', '--verbose=4', appPath]);
  const output = `${result.stdout || ''}${result.stderr || ''}`.trim();

  if (result.status === 0) {
    note(`${label}: Gatekeeper accepted this app (${output || 'no spctl detail'}).`);
    return;
  }

  const normalized = output.toLowerCase();
  if (normalized.includes('not signed at all') || normalized.includes('invalid signature')) {
    fail(`${label}: Gatekeeper rejection indicates a broken signature: ${output}`);
    return;
  }

  note(`${label}: Gatekeeper rejected the app as expected for ad-hoc, unnotarized distribution: ${output || `exit ${result.status}`}`);
}

function reportQuarantine(targetPath, label) {
  const result = runResult('xattr', ['-p', 'com.apple.quarantine', targetPath]);
  if (result.status === 0) {
    fail(`${label}: build artifact unexpectedly has com.apple.quarantine=${result.stdout.trim()}`);
  } else {
    ok(`${label}: no quarantine xattr on generated artifact`);
  }
}

function verifyApp(appPath, label) {
  const resolved = path.resolve(appPath);
  if (!assertAppPath(resolved, label)) {
    return;
  }

  reportQuarantine(resolved, label);
  verifyCodeSignature(resolved, label, { deep: true, requireAdhoc: true });
  verifyMachOObjects(resolved, label);
  assessGatekeeper(resolved, label);
}

function attachDmg(dmgPath) {
  const mountPoint = fs.mkdtempSync(path.join(os.tmpdir(), 'firelink-dmg-'));
  try {
    const plist = run('hdiutil', [
      'attach',
      '-plist',
      '-nobrowse',
      '-readonly',
      '-mountpoint',
      mountPoint,
      dmgPath,
    ], { timeout: 60000 });
    return { mountPoint, plist };
  } catch (error) {
    fs.rmSync(mountPoint, { recursive: true, force: true });
    throw error;
  }
}

function detachDmg(mountPoint) {
  const result = runResult('hdiutil', ['detach', mountPoint]);
  if (result.status !== 0) {
    note(`Initial hdiutil detach failed, retrying with -force: ${result.stderr?.trim() || result.stdout?.trim()}`);
    const forced = runResult('hdiutil', ['detach', '-force', mountPoint]);
    if (forced.status !== 0) {
      fail(`Failed to detach DMG mount point ${mountPoint}: ${forced.stderr?.trim() || forced.stdout?.trim()}`);
    }
  }
  fs.rmSync(mountPoint, { recursive: true, force: true });
}

function verifyDmg(dmgPath) {
  const resolved = path.resolve(dmgPath);
  if (!fs.existsSync(resolved)) {
    fail(`DMG does not exist at ${resolved}`);
    return;
  }

  reportQuarantine(resolved, 'DMG');
  let mount;
  try {
    mount = attachDmg(resolved);
    ok(`DMG mounted at ${mount.mountPoint}`);
    const apps = fs.readdirSync(mount.mountPoint)
      .filter(name => name.endsWith('.app'))
      .map(name => path.join(mount.mountPoint, name));
    if (apps.length !== 1) {
      fail(`Expected exactly one .app inside DMG, found ${apps.length}`);
      return;
    }
    verifyApp(apps[0], 'DMG app');
  } catch (error) {
    fail(`DMG verification failed: ${error.stderr?.trim() || error.message}`);
  } finally {
    if (mount) {
      detachDmg(mount.mountPoint);
    }
  }
}

if (appArg) {
  verifyApp(appArg, 'Built app');
}

if (dmgArg) {
  verifyDmg(dmgArg);
}

console.log('');
if (exitCode !== 0) {
  console.error('[FAIL] macOS signing verification failed.');
  process.exit(1);
}

console.log('[PASS] macOS ad-hoc signing verification passed.');
