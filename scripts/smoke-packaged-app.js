#!/usr/bin/env node
import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

function argValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const executableArg = argValue('--executable');
if (!executableArg) {
  console.error('Pass --executable <path>.');
  process.exit(1);
}

const executable = path.resolve(executableArg);
const assertNoVisibleChildWindows = process.argv.includes('--assert-no-visible-child-windows');
const assertPortableData = process.argv.includes('--assert-portable-data');
const child = spawn(executable, [], {
  cwd: process.env.RUNNER_TEMP || process.env.TMPDIR || process.cwd(),
  detached: process.platform !== 'win32',
  env: {
    ...process.env,
    FIRELINK_SMOKE_TEST: '1',
    WEBKIT_DISABLE_COMPOSITING_MODE: '1',
    GDK_BACKEND: 'x11',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
});

let stderr = '';
let spawnError = null;
let readyPort = null;
let childExit = null;

child.on('error', error => {
  spawnError = error;
});

child.on('exit', (code, signal) => {
  childExit = { code, signal };
  if (readyPort === null) {
    console.error(`Child exited prematurely with code ${code} signal ${signal}`);
  }
});

child.stderr.on('data', data => {
  stderr += data.toString();
});
child.stdout.on('data', () => {});

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function findReadyPort() {
  for (let attempt = 0; attempt < 200 && readyPort === null; attempt += 1) {
    if (spawnError || childExit) {
      break;
    }

    for (let port = 6412; port <= 6422; port += 1) {
      try {
        const response = await fetch(`http://127.0.0.1:${port}/ping`);
        if (
          response.headers.get('x-firelink-server') === '1'
          && response.headers.get('x-firelink-smoke-process-id') === String(child.pid)
        ) {
          readyPort = port;
          break;
        }
      } catch {}
    }

    if (readyPort === null) {
      await sleep(250);
    }
  }
}

function assertNoVisibleWindows(rootPid) {
  if (process.platform !== 'win32') {
    return;
  }

  const script = `
$root = ${rootPid}
$all = Get-CimInstance Win32_Process
$pending = @($root)
$descendants = @()
while ($pending.Count -gt 0) {
  $parent = $pending[0]
  if ($pending.Count -eq 1) {
    $pending = @()
  } else {
    $pending = $pending[1..($pending.Count - 1)]
  }

  $children = @($all | Where-Object { $_.ParentProcessId -eq $parent })
  foreach ($child in $children) {
    $descendants += $child.ProcessId
    $pending += $child.ProcessId
  }
}

$visible = foreach ($childPid in $descendants) {
  $process = Get-Process -Id $childPid -ErrorAction SilentlyContinue
  if ($process -and $process.MainWindowHandle -ne 0) {
    "$($process.ProcessName)($childPid)"
  }
}

if ($visible.Count -gt 0) {
  Write-Error "Visible child process windows detected: $($visible -join ', ')"
  exit 1
}
`;

  try {
    execFileSync('powershell', ['-NoProfile', '-Command', script], {
      stdio: 'pipe',
      windowsHide: true,
    });
  } catch (error) {
    const detail = [error.stdout?.toString(), error.stderr?.toString()].filter(Boolean).join('\n').trim();
    throw new Error(detail || 'Visible child process window check failed.');
  }
}

function waitForChildExit(timeoutMs) {
  if (childExit) {
    return Promise.resolve(true);
  }

  return new Promise(resolve => {
    const timer = setTimeout(() => {
      resolve(false);
    }, timeoutMs);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

async function terminateChild() {
  if (!child.pid || childExit) {
    return true;
  }

  if (process.platform === 'win32') {
    try {
      execFileSync('taskkill', ['/pid', String(child.pid), '/t', '/f'], {
        stdio: 'ignore',
        windowsHide: true,
      });
    } catch {}
    return waitForChildExit(10000);
  }

  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch {
    child.kill('SIGTERM');
  }
  if (await waitForChildExit(5000)) {
    return true;
  }

  try {
    process.kill(-child.pid, 'SIGKILL');
  } catch {
    child.kill('SIGKILL');
  }
  return waitForChildExit(5000);
}

function assertPortableStorage() {
  const portableRoot = path.dirname(executable);
  const marker = path.join(portableRoot, 'portable.flag');
  const database = path.join(portableRoot, 'data', 'firelink.sqlite');
  const webviewData = path.join(portableRoot, 'data', 'webview');

  if (!fs.statSync(marker, { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`Portable marker was not found at ${marker}`);
  }
  if (!fs.statSync(database, { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`Portable database was not created at ${database}`);
  }
  if (!fs.statSync(webviewData, { throwIfNoEntry: false })?.isDirectory()) {
    throw new Error(`Portable WebView data directory was not created at ${webviewData}`);
  }
}

try {
  await findReadyPort();

  if (readyPort === null) {
    if (spawnError) {
      throw new Error(`Packaged Firelink failed to start: ${spawnError.message}`);
    } else if (childExit) {
      throw new Error(
        `Packaged Firelink exited before exposing extension ping endpoint with code ${childExit.code} signal ${childExit.signal}.`,
      );
    } else {
      throw new Error(`Packaged Firelink did not expose extension ping endpoint. Stderr:\n${stderr.slice(-1000)}`);
    }
  }

  if (assertNoVisibleChildWindows) {
    assertNoVisibleWindows(child.pid);
  }
  if (assertPortableData) {
    assertPortableStorage();
  }

  if (childExit) {
    throw new Error(`Packaged Firelink exited after exposing extension ping endpoint with code ${childExit.code} signal ${childExit.signal}.`);
  }

  console.log(`Packaged Firelink smoke passed on 127.0.0.1:${readyPort}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
} finally {
  if (!await terminateChild()) {
    console.error('Packaged Firelink could not be terminated cleanly; refusing to report smoke success.');
    process.exitCode = 1;
  }
}
