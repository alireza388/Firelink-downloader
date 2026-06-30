#!/usr/bin/env node
import { execFileSync, spawn } from 'node:child_process';
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

child.on('error', error => {
  spawnError = error;
});

child.on('exit', (code, signal) => {
  if (readyPort === null) {
    console.error(`Child exited prematurely with code ${code} signal ${signal}`);
  }
});

child.stderr.on('data', data => {
  stderr += data.toString();
});

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function findReadyPort() {
  for (let attempt = 0; attempt < 200 && readyPort === null; attempt += 1) {
    if (spawnError) {
      break;
    }

    for (let port = 6412; port <= 6422; port += 1) {
      try {
        const response = await fetch(`http://127.0.0.1:${port}/ping`);
        if (response.headers.get('x-firelink-server') === '1') {
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

$visible = foreach ($pid in $descendants) {
  $process = Get-Process -Id $pid -ErrorAction SilentlyContinue
  if ($process -and $process.MainWindowHandle -ne 0) {
    "$($process.ProcessName)($pid)"
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

function terminateChild() {
  if (!child.pid) {
    return;
  }

  if (process.platform === 'win32') {
    spawn('taskkill', ['/pid', String(child.pid), '/t', '/f'], {
      stdio: 'ignore',
      windowsHide: true,
    });
    return;
  }

  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch {
    child.kill('SIGTERM');
  }
}

await findReadyPort();

try {
  if (readyPort === null) {
    if (spawnError) {
      console.error(`Packaged Firelink failed to start: ${spawnError.message}`);
    } else {
      console.error(`Packaged Firelink did not expose extension ping endpoint. Stderr:\n${stderr.slice(-1000)}`);
    }
    process.exit(1);
  }

  if (assertNoVisibleChildWindows) {
    assertNoVisibleWindows(child.pid);
  }

  console.log(`Packaged Firelink smoke passed on 127.0.0.1:${readyPort}`);
} finally {
  terminateChild();
}
