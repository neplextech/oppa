import { createHash } from 'node:crypto';
import { readFile, stat, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

/**
 * Builds the `latest-android.json` manifest read by the Android update check.
 *
 * The app only offers a newer semantic version, verifies the APK's size and
 * SHA-256 after an HTTPS download, and then requires the APK to carry the
 * installed app's package name and signing certificate.
 */
export function buildAndroidManifest({ version, url, sha256, size, notes, pubDate }) {
  if (!SEMVER.test(version)) throw new Error('Android releases require a stable semantic version.');
  const parsed = new URL(url);
  if (parsed.protocol !== 'https:' || parsed.username || parsed.password) {
    throw new Error('The APK URL must use HTTPS without credentials.');
  }
  if (!/^[0-9a-f]{64}$/.test(sha256)) throw new Error('The APK checksum must be a lowercase SHA-256 digest.');
  if (!Number.isSafeInteger(size) || size <= 0) throw new Error('The APK size must be a positive integer.');
  return {
    version,
    notes: notes ?? null,
    pub_date: pubDate,
    android: { url, sha256, size },
  };
}

/** Hashes a file and returns its lowercase SHA-256 digest and size. */
export async function describeFile(path) {
  const bytes = await readFile(path);
  return { sha256: createHash('sha256').update(bytes).digest('hex'), size: (await stat(path)).size };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [apkPath, outputPath] = process.argv.slice(2);
  if (!apkPath || !outputPath) {
    throw new Error('Usage: node android-update-manifest.mjs <apk> <output.json>');
  }
  const { version, repository, tag, assetName, notes } = {
    version: process.env.OPPA_VERSION,
    repository: process.env.GITHUB_REPOSITORY,
    tag: process.env.OPPA_RELEASE_TAG,
    assetName: process.env.OPPA_APK_NAME,
    notes: process.env.OPPA_RELEASE_NOTES,
  };
  const file = await describeFile(apkPath);
  const manifest = buildAndroidManifest({
    version,
    url: `https://github.com/${repository}/releases/download/${tag}/${assetName}`,
    ...file,
    notes,
    pubDate: new Date().toISOString(),
  });
  await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath} for ${assetName} (${file.size} bytes).`);
}
