import assert from 'node:assert/strict';
import { test } from 'node:test';

import { createDownloadRelease, findLatestStableOppaRelease } from '../lib/github-releases.ts';

const checksum = 'a'.repeat(64);
const asset = (id, name) => ({
  id,
  name,
  browserDownloadUrl: `https://github.com/neplextech/oppa/releases/download/oppa-v0.4.0/${name}`,
  size: 1234,
  contentType: 'application/octet-stream',
  digest: `sha256:${checksum}`,
});
const release = {
  id: 1,
  tagName: 'oppa-v0.4.0',
  name: 'OPPA 0.4.0',
  body: '',
  htmlUrl: 'https://github.com/neplextech/oppa/releases/tag/oppa-v0.4.0',
  publishedAt: '2026-11-26T00:00:00Z',
  createdAt: '2026-11-26T00:00:00Z',
  draft: false,
  prerelease: false,
  assets: [
    asset(1, 'OPPA_0.4.0_android_universal.apk'),
    asset(2, 'OPPA_0.4.0_aarch64.dmg'),
    asset(3, 'latest-android.json'),
  ],
};

test('offers published Android APKs alongside desktop installers with verified digests', () => {
  const downloads = createDownloadRelease(release);
  assert.equal(downloads.assetsByPlatform.Android.length, 1);
  assert.equal(downloads.assetsByPlatform.Android[0].architecture, 'universal');
  assert.equal(downloads.assetsByPlatform.Android[0].sha256, checksum);
  assert.equal(downloads.assetsByPlatform.macOS.length, 1);
  assert.equal(downloads.assetsByPlatform.Linux.length, 0);
});

test('recognizes APK releases and excludes drafts and prereleases', () => {
  const android = { ...release, assets: [release.assets[0]] };
  assert.equal(findLatestStableOppaRelease([android]), android);
  assert.equal(findLatestStableOppaRelease([{ ...android, draft: true }]), null);
  assert.equal(findLatestStableOppaRelease([{ ...android, prerelease: true }]), null);
});
