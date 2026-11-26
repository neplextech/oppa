import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { patchIosBuildPhase } from '../scripts/ios-xcode-script.mjs';

const original =
  'pnpm tauri ios xcode-script -v --platform "${PLATFORM_DISPLAY_NAME:?}" --configuration "${CONFIGURATION:?}"';

test('the Xcode build phase calls the installed CLI bridge and preserves its arguments', () => {
  const directory = mkdtempSync(join(tmpdir(), 'oppa-ios-build-'));
  const bridge = join(directory, "bridge's $literal.mjs");
  try {
    writeFileSync(bridge, 'process.stdout.write(JSON.stringify(process.argv.slice(2)));');
    const source = `shellScript = ${JSON.stringify(original)};`;
    const patched = patchIosBuildPhase(source, bridge, true);
    const script = JSON.parse(patched.slice('shellScript = '.length, -1));
    assert.ok(!script.includes('pnpm'));
    const result = spawnSync('/bin/sh', ['-c', script], {
      env: { ...process.env, PLATFORM_DISPLAY_NAME: 'iOS Simulator', CONFIGURATION: 'debug' },
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), ['-v', '--platform', 'iOS Simulator', '--configuration', 'debug']);
    assert.equal(patchIosBuildPhase(patched, bridge, true), patched);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('XcodeGen keeps the direct bridge command when regenerating the project', () => {
  const source = `      - script: ${original}\n        name: Build Rust Code`;
  const patched = patchIosBuildPhase(source, '/project/scripts/ios-xcode-script.mjs', false);
  assert.ok(patched.includes("script: node '/project/scripts/ios-xcode-script.mjs' -v --platform"));
  assert.ok(patched.includes('name: Build Rust Code'));
  assert.equal(patchIosBuildPhase(patched, '/project/scripts/ios-xcode-script.mjs', false), patched);
});
