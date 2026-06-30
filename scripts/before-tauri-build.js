#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');

function run(command, args) {
  const executable = process.platform === 'win32' && command === 'npm' ? 'npm.cmd' : command;
  const result = spawnSync(executable, args, {
    cwd: repoRoot,
    stdio: 'inherit',
  });

  if (result.error) {
    console.error(`Failed to run ${executable}: ${result.error.message}`);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run(process.execPath, ['scripts/stage-engines.js']);
run(process.execPath, ['scripts/verify-binaries.js', '--staged']);

if (process.env.FIRELINK_OMIT_ENGINE_DIST_FOR_TAURI_BUNDLE === '1') {
  const engineDist = path.join(repoRoot, 'src-tauri', 'engine-dist');
  fs.rmSync(engineDist, { recursive: true, force: true });
  fs.mkdirSync(engineDist, { recursive: true });
  console.log('Omitted engine-dist from the initial Tauri bundle; release packaging will repack verified engines.');
}

run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'build']);
