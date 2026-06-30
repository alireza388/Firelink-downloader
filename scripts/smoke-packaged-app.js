#!/usr/bin/env node
import { spawn } from 'node:child_process';
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

const child = spawn(executable, [], {
  cwd: process.env.RUNNER_TEMP || process.env.TMPDIR || process.cwd(),
  detached: process.platform !== 'win32',
  env: { 
    ...process.env, 
    FIRELINK_SMOKE_TEST: '1',
    WEBKIT_DISABLE_COMPOSITING_MODE: '1',
    GDK_BACKEND: 'x11'
  },
  stdio: ['ignore', 'pipe', 'pipe']
});
let stderr = '';
let spawnError = null;
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

let readyPort = null;
for (let attempt = 0; attempt < 200 && readyPort === null; attempt += 1) {
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
    await new Promise(resolve => setTimeout(resolve, 250));
  }
}

if (process.platform === 'win32') {
  spawn('taskkill', ['/pid', String(child.pid), '/t', '/f'], { stdio: 'ignore' });
} else {
  try {
    process.kill(-child.pid, 'SIGTERM');
  } catch {
    child.kill('SIGTERM');
  }
}

if (readyPort === null) {
  const detail = spawnError?.message || stderr.slice(-1000);
  console.error(`Packaged Firelink did not expose extension server. ${detail}`);
  process.exit(1);
}
console.log(`Packaged Firelink smoke passed on 127.0.0.1:${readyPort}`);
