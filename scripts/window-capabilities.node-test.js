import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';

const capability = JSON.parse(fs.readFileSync('src-tauri/capabilities/default.json', 'utf8'));
const permissions = new Set(capability.permissions);

test('custom window controls have required Tauri permissions', () => {
  const windowControls = fs.readFileSync('src/components/WindowControls.tsx', 'utf8');
  const requiredPermissions = new Map([
    ['.close()', 'core:window:allow-close'],
    ['.minimize()', 'core:window:allow-minimize'],
    ['.startDragging()', 'core:window:allow-start-dragging'],
    ['.toggleMaximize()', 'core:window:allow-toggle-maximize'],
  ]);

  for (const [apiCall, permission] of requiredPermissions) {
    if (windowControls.includes(apiCall)) {
      assert.equal(
        permissions.has(permission),
        true,
        `${apiCall} requires ${permission} in src-tauri/capabilities/default.json`
      );
    }
  }
});
