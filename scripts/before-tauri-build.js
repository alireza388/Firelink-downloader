#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');

function run(command, args, options = {}) {
  const executable = process.platform === 'win32' && command === 'npm' ? 'npm.cmd' : command;
  const result = spawnSync(executable, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    ...options,
  });

  if (result.error) {
    console.error(`Failed to run ${executable}: ${result.error.message}`);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function runNpmScript(script) {
  if (process.env.npm_execpath) {
    run(process.execPath, [process.env.npm_execpath, 'run', script]);
    return;
  }

  if (process.platform === 'win32') {
    run('cmd.exe', ['/d', '/s', '/c', 'npm', 'run', script], {
      windowsHide: true,
    });
    return;
  }

  run('npm', ['run', script]);
}

run(process.execPath, ['scripts/stage-engines.js']);
run(process.execPath, ['scripts/verify-binaries.js', '--staged']);

if (process.env.FIRELINK_OMIT_ENGINE_DIST_FOR_TAURI_BUNDLE === '1') {
  const engineDist = path.join(repoRoot, 'src-tauri', 'engine-dist');
  fs.rmSync(engineDist, { recursive: true, force: true });
  fs.mkdirSync(engineDist, { recursive: true });
  console.log('Omitted engine-dist from the initial Tauri bundle; release packaging will repack verified engines.');
}

runNpmScript('build');
