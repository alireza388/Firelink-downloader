#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { execFileSync, spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const archMap = { x64: 'x86_64', arm64: 'aarch64' };
const platformMap = {
  darwin: 'apple-darwin',
  win32: 'pc-windows-msvc',
  linux: 'unknown-linux-gnu',
};

const currentArch = archMap[os.arch()];
const currentPlatform = platformMap[os.platform()];

if (!currentArch || !currentPlatform) {
  console.error(`Unsupported architecture or platform: ${os.arch()} / ${os.platform()}`);
  process.exit(1);
}

const targetTriple = `${currentArch}-${currentPlatform}`;
const isWindows = os.platform() === 'win32';
const isMacOS = os.platform() === 'darwin';
const ext = isWindows ? '.exe' : '';
const suffix = `-${targetTriple}${ext}`;

const scriptsDir = __dirname;
const binariesDir = path.join(scriptsDir, '..', 'src-tauri', 'binaries');
const requiredEngines = ['yt-dlp', 'aria2c', 'ffmpeg', 'deno'];

const FORBIDDEN_OTOOL_PATHS = ['/opt/homebrew', '/usr/local/Cellar'];
const FORBIDDEN_STDERR = [
  'Failed to load Python shared library',
  'Library not loaded',
  'image not found',
  'Connection refused',
];

let exitCode = 0;

function fail(msg) {
  console.error(`[FAIL] ${msg}`);
  exitCode = 1;
}

function ok(msg) {
  console.log(`[OK] ${msg}`);
}

function binName(engine) {
  return `${engine}${suffix}`;
}

function binPath(engine) {
  return path.join(binariesDir, binName(engine));
}

// ───── Check 1: Sidecar existence ─────
console.log(`\n─── 1. Sidecar existence (${targetTriple}) ───`);
for (const eng of requiredEngines) {
  const p = binPath(eng);
  if (fs.existsSync(p)) {
    ok(`Found ${binName(eng)}`);
  } else {
    fail(`Missing ${binName(eng)}`);
  }
}

if (exitCode !== 0) {
  console.error('\nAborting: missing required sidecars.');
  process.exit(1);
}

// ───── Check 2: Executable permission ─────
console.log('\n─── 2. Executable permission ───');
for (const eng of requiredEngines) {
  try {
    fs.accessSync(binPath(eng), fs.constants.X_OK);
    ok(`Executable ${binName(eng)}`);
  } catch {
    fail(`Not executable ${binName(eng)}`);
  }
}

// ───── Check 3: file(1) identification ─────
console.log('\n─── 3. file(1) identification ───');
for (const eng of requiredEngines) {
  try {
    const out = execFileSync('file', ['--brief', binPath(eng)], {
      encoding: 'utf-8',
      timeout: 5000,
    }).trim();
    ok(`${binName(eng)}: ${out}`);
  } catch (e) {
    fail(`file ${binName(eng)}: ${e.message}`);
  }
}

// ───── Check 4 & 5: otool -L linkage (macOS only) ─────
if (isMacOS) {
  console.log('\n─── 4 & 5. otool -L linkage check ───');
  for (const eng of requiredEngines) {
    const p = binPath(eng);
    try {
      const out = execFileSync('otool', ['-L', p], {
        encoding: 'utf-8',
        timeout: 10000,
      });
      const lines = out
        .split('\n')
        .slice(1)
        .map((l) => l.trim())
        .filter(Boolean);
      let hasBad = false;
      for (const fp of FORBIDDEN_OTOOL_PATHS) {
        for (const line of lines) {
          if (line.includes(fp)) {
            fail(`${binName(eng)} links to '${fp}': ${line}`);
            hasBad = true;
          }
        }
      }
      if (!hasBad) {
        ok(`${binName(eng)}: no local-only dylib paths`);
      }
    } catch (e) {
      fail(`otool -L ${binName(eng)}: ${e.message}`);
    }
  }
}

// ───── Check 6: yt-dlp packaging ─────
console.log('\n─── 6. yt-dlp packaging ───');
{
  const yt = binPath('yt-dlp');
  if (!fs.existsSync(yt)) {
    fail('yt-dlp binary not found, cannot verify packaging');
  } else {
    const internalDir = path.join(binariesDir, '_internal');
    const hasInternal = fs.existsSync(internalDir) && fs.statSync(internalDir).isDirectory();

    if (hasInternal) {
      console.log('  Detected PyInstaller onedir layout (_internal/ present)');
      let entries;
      try {
        entries = fs.readdirSync(internalDir);
        ok(`_internal/ directory exists with ${entries.length} entries`);
      } catch (e) {
        fail(`Cannot read _internal/: ${e.message}`);
      }
      if (entries) {
        let broken = 0;
        for (const entry of entries) {
          const ep = path.join(internalDir, entry);
          if (fs.lstatSync(ep).isSymbolicLink()) {
            try {
              fs.accessSync(ep);
            } catch {
              fail(`Broken symlink in _internal/: ${entry}`);
              broken++;
            }
          }
        }
        if (broken === 0) {
          ok('_internal/ symlinks valid');
        }
      }
    } else {
      console.log('  No _internal/ directory, assuming standalone onefile binary');
    }
  }
}

// ───── Check 7, 8 & 9: Engine version self-tests ─────
console.log('\n─── 7 & 8 & 9. Engine version self-tests ───');

function runEngine(label, engine, args, timeout = 30000) {
  const p = binPath(engine);
  if (!fs.existsSync(p)) {
    fail(`${label} binary not found at ${p}`);
    return;
  }
  let stderr = '';
  try {
    const stdout = execFileSync(p, args, {
      encoding: 'utf-8',
      timeout,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const firstLine = stdout.trim().split('\n')[0];
    ok(`${label} version: ${firstLine}`);
  } catch (e) {
    stderr = e.stderr || '';
    const stdout = e.stdout || '';
    const detail = stdout.trim().split('\n')[0] || e.message;
    fail(`${label} execution failed: ${detail}`);
  }
  if (stderr) {
    for (const pattern of FORBIDDEN_STDERR) {
      if (stderr.includes(pattern)) {
        fail(`${label} stderr contains '${pattern}'`);
      }
    }
  }
}

runEngine('yt-dlp', 'yt-dlp', ['--version'], 45000);
runEngine('ffmpeg', 'ffmpeg', ['-version']);
runEngine('deno', 'deno', ['--version']);
runEngine('aria2c', 'aria2c', ['--version']);

// ───── aria2 RPC smoke test (macOS only) ─────
if (isMacOS) {
  console.log('\n─── aria2 RPC smoke test ───');
  await (async function testAria2Rpc() {
    const p = binPath('aria2c');
    if (!fs.existsSync(p)) {
      fail('aria2c binary not found, cannot run RPC test');
      return;
    }

    const port = 16801 + (process.pid % 1000);
    const proc = spawn(p, [
      '--enable-rpc',
      `--rpc-listen-port=${port}`,
      '--rpc-max-request-size=1K',
      '--quiet',
      '--console-log-level=error',
      '--rpc-listen-all=false',
    ], {
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout: 15000,
    });

    let rpcStderr = '';
    proc.stderr.on('data', (d) => {
      rpcStderr += d.toString();
    });

    const body = JSON.stringify({
      jsonrpc: '2.0',
      id: 'firelink-verify',
      method: 'aria2.getVersion',
      params: [],
    });

    const result = await new Promise((resolve) => {
      const maxAttempts = 20;
      let attempts = 0;

      function tryFetch() {
        attempts++;
        fetch(`http://127.0.0.1:${port}/jsonrpc`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body,
        })
          .then(async (res) => {
            resolve({ ok: true, data: await res.text() });
          })
          .catch(() => {
            if (attempts >= maxAttempts) {
              resolve({ ok: false, error: `RPC not ready after ${maxAttempts} attempts` });
              return;
            }
            setTimeout(tryFetch, 300);
          });
      }

      tryFetch();
    });

    // Clean up
    proc.kill('SIGTERM');
    setTimeout(() => {
      try {
        proc.kill('SIGKILL');
      } catch {}
    }, 2000);

    if (result.ok) {
      try {
        const resp = JSON.parse(result.data);
        if (resp?.result?.version) {
          ok(`aria2 RPC version: ${resp.result.version}`);
        } else {
          fail(`aria2 RPC unexpected response: ${result.data}`);
        }
      } catch (e) {
        fail(`aria2 RPC parse error: ${e.message}`);
      }
    } else {
      fail(result.error);
    }

    if (rpcStderr) {
      for (const pattern of FORBIDDEN_STDERR) {
        if (rpcStderr.includes(pattern)) {
          fail(`aria2 RPC stderr contains '${pattern}'`);
        }
      }
    }
  })();
}

// ───── Result ─────
console.log('');
if (exitCode !== 0) {
  console.error(`[FAIL] ${exitCode} engine verification check(s) failed.`);
  process.exit(1);
} else {
  console.log('[PASS] All engine verification checks passed.');
  process.exit(0);
}
