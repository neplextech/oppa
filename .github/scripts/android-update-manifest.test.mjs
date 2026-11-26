import assert from 'node:assert/strict';
import { test } from 'node:test';

import { buildAndroidManifest } from './android-update-manifest.mjs';

const valid = {
  version: '0.4.0',
  url: 'https://github.com/neplextech/oppa/releases/download/oppa-v0.4.0/OPPA_0.4.0_android.apk',
  sha256: 'a'.repeat(64),
  size: 1024,
  notes: 'Fixes',
  pubDate: '2026-11-26T00:00:00.000Z',
};

test('builds the manifest shape read by the Android app', () => {
  assert.deepEqual(buildAndroidManifest(valid), {
    version: '0.4.0',
    notes: 'Fixes',
    pub_date: '2026-11-26T00:00:00.000Z',
    android: { url: valid.url, sha256: valid.sha256, size: 1024 },
  });
});

test('rejects manifests the app would refuse', () => {
  assert.throws(() => buildAndroidManifest({ ...valid, version: '0.4.0-beta.1' }));
  assert.throws(() => buildAndroidManifest({ ...valid, url: 'http://example.com/a.apk' }));
  assert.throws(() => buildAndroidManifest({ ...valid, sha256: 'A'.repeat(64) }));
  assert.throws(() => buildAndroidManifest({ ...valid, size: 0 }));
});
