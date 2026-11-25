import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Checks matching stable desktop versions and refuses to reuse a published release. */
export async function checkDesktopRelease(
  { frontendVersion, desktopVersion, repository, apiUrl = 'https://api.github.com', token },
  request = fetch,
) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(desktopVersion)) {
    throw new Error('Desktop releases require a stable semantic version.');
  }
  if (frontendVersion !== desktopVersion) {
    throw new Error('Desktop package and Tauri versions must match.');
  }
  if (!repository || !token) throw new Error('Release checks require a repository and GitHub token.');
  const tag = `oppa-v${desktopVersion}`;
  const response = await request(`${apiUrl}/repos/${repository}/releases/tags/${tag}`, {
    headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json' },
    signal: AbortSignal.timeout(15_000),
  });
  if (response.status === 404) return tag;
  if (!response.ok) throw new Error(`Cannot verify ${tag}: GitHub returned HTTP ${response.status}.`);
  const release = await response.json();
  if (release?.draft !== true || release?.immutable === true) {
    throw new Error(`${tag} is already published or immutable. Bump both desktop manifests before releasing again.`);
  }
  return tag;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const frontend = JSON.parse(await readFile(new URL('../../apps/oppa/package.json', import.meta.url), 'utf8'));
  const desktop = JSON.parse(
    await readFile(new URL('../../apps/oppa/src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
  );
  const tag = await checkDesktopRelease({
    frontendVersion: frontend.version,
    desktopVersion: desktop.version,
    repository: process.env.GITHUB_REPOSITORY,
    apiUrl: process.env.GITHUB_API_URL,
    token: process.env.GH_TOKEN,
  });
  console.log(`${tag} is available for draft release builds.`);
}
