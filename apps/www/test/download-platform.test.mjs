import assert from 'node:assert/strict';
import { test } from 'node:test';

import { detectDownloadPlatform, pickRecommendedAsset } from '../lib/download-platform.ts';
import { createDownloadRelease } from '../lib/github-releases.ts';

test('Android phones and tablets are recognized before Linux', () => {
  for (const userAgent of [
    'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 Chrome/140.0 Mobile Safari/537.36',
    'Mozilla/5.0 (Linux; Android 14; Tablet) AppleWebKit/537.36 Chrome/140.0 Safari/537.36',
  ])
    assert.equal(detectDownloadPlatform({ userAgent }), 'Android');
  assert.equal(
    detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (X11; Linux x86_64)', userAgentData: { platform: 'Android' } }),
    'Android',
  );
});

test('desktop suggestions remain available and iOS does not suggest a Mac installer', () => {
  assert.equal(detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' }), 'Windows');
  assert.equal(detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (X11; Linux x86_64)' }), 'Linux');
  assert.equal(detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' }), 'macOS');
  assert.equal(
    detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)', maxTouchPoints: 5 }),
    null,
  );
  assert.equal(detectDownloadPlatform({ userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS)' }), null);
  assert.equal(detectDownloadPlatform({ userAgent: '' }), null);
});

test('Android recommends the universal APK even when an x64 build is present', () => {
  const assets = createDownloadRelease({
    assets: [
      { id: 1, name: 'OPPA_android_x64.apk', digest: null },
      { id: 2, name: 'OPPA_android_universal.apk', digest: null },
      { id: 3, name: 'OPPA_aarch64.dmg', digest: null },
    ],
  }).assetsByPlatform;
  assert.equal(pickRecommendedAsset(Object.values(assets).flat(), 'Android').id, 2);
  assert.equal(pickRecommendedAsset(assets.macOS, 'Android'), null);
});
