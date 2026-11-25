import assert from 'node:assert/strict';
import { test } from 'node:test';

import { checkDesktopRelease } from './check-desktop-release.mjs';

const config = {
  frontendVersion: '0.3.1',
  desktopVersion: '0.3.1',
  repository: 'example/agent',
  token: 'test-only',
};
const reply = (status, body) => async () => new Response(JSON.stringify(body), { status });

test('a missing version or existing mutable draft permits release builds', async () => {
  assert.equal(await checkDesktopRelease(config, reply(404, {})), 'oppa-v0.3.1');
  assert.equal(await checkDesktopRelease(config, reply(200, { draft: true, immutable: false })), 'oppa-v0.3.1');
});

test('published and immutable releases are rejected before asset uploads', async () => {
  for (const metadata of [{ draft: false, immutable: true }, { draft: false }, { draft: true, immutable: true }, {}]) {
    await assert.rejects(checkDesktopRelease(config, reply(200, metadata)), /Bump both desktop manifests/);
  }
});

test('registry access and network failures do not become missing releases', async () => {
  for (const status of [401, 403, 429, 500]) {
    await assert.rejects(checkDesktopRelease(config, reply(status, {})), /Cannot verify/);
  }
  await assert.rejects(
    checkDesktopRelease(config, async () => {
      throw new Error('network interrupted');
    }),
    /network interrupted/,
  );
});

test('manifest mismatch and prereleases fail before making a request', async () => {
  const unexpected = async () => {
    throw new Error('Unexpected request');
  };
  await assert.rejects(checkDesktopRelease({ ...config, frontendVersion: '0.3.0' }, unexpected), /must match/);
  await assert.rejects(
    checkDesktopRelease({ ...config, desktopVersion: '0.3.1-beta.1' }, unexpected),
    /stable semantic version/,
  );
});
