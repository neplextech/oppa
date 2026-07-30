export const GITHUB_RELEASES_URL = 'https://api.github.com/repos/neplextech/oppa/releases?per_page=20';
export const GITHUB_RELEASES_PAGE_URL = 'https://github.com/neplextech/oppa/releases';
export const RELEASE_CACHE_KEY = 'oppa.github-releases.v1';
export const RELEASE_REFRESH_BACKOFF_KEY = 'oppa.github-releases.refresh-backoff.v1';
export const RELEASE_CACHE_TTL_MS = 15 * 60 * 1000;
export const RELEASE_REFRESH_BACKOFF_MS = 60 * 1000;
export const RELEASE_CACHE_VERSION = 1;

export type DownloadPlatform = 'macOS' | 'Windows' | 'Linux';
export type DownloadArchitecture = 'universal' | 'arm64' | 'x64' | 'x86' | 'armv7' | 'unknown';

export interface GitHubReleaseAsset {
  id: number;
  name: string;
  browserDownloadUrl: string;
  size: number;
  contentType: string;
  digest: string | null;
}

export interface GitHubRelease {
  id: number;
  tagName: string;
  name: string | null;
  body: string | null;
  htmlUrl: string;
  publishedAt: string | null;
  createdAt: string;
  draft: boolean;
  prerelease: boolean;
  assets: GitHubReleaseAsset[];
}

export interface CachedReleaseData {
  version: typeof RELEASE_CACHE_VERSION;
  cachedAt: number;
  releases: GitHubRelease[];
  checksumsByTag: Record<string, Record<string, string>>;
}

export interface DownloadAsset extends GitHubReleaseAsset {
  platform: DownloadPlatform;
  architecture: DownloadArchitecture;
  format: string;
  architectureLabel: string;
  sha256: string | null;
}

export interface DownloadRelease {
  release: GitHubRelease;
  assetsByPlatform: Record<DownloadPlatform, DownloadAsset[]>;
  checksumAsset: GitHubReleaseAsset | null;
}

type UnknownRecord = Record<string, unknown>;

const INSTALLER_EXTENSIONS = ['.dmg', '.exe', '.msi', '.appimage', '.deb', '.rpm'] as const;
const PLATFORM_ORDER: DownloadPlatform[] = ['macOS', 'Windows', 'Linux'];
const ARCHITECTURE_ORDER: DownloadArchitecture[] = ['universal', 'arm64', 'x64', 'armv7', 'x86', 'unknown'];

function isRecord(value: unknown): value is UnknownRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readString(record: UnknownRecord, key: string): string {
  const value = record[key];
  if (typeof value !== 'string') {
    throw new Error(`GitHub release data is missing a valid ${key} field.`);
  }
  return value;
}

function readNullableString(record: UnknownRecord, key: string): string | null {
  const value = record[key];
  if (value === null) return null;
  if (typeof value !== 'string') {
    throw new Error(`GitHub release data is missing a valid ${key} field.`);
  }
  return value;
}

function readNumber(record: UnknownRecord, key: string): number {
  const value = record[key];
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new Error(`GitHub release data is missing a valid ${key} field.`);
  }
  return value;
}

function readBoolean(record: UnknownRecord, key: string): boolean {
  const value = record[key];
  if (typeof value !== 'boolean') {
    throw new Error(`GitHub release data is missing a valid ${key} field.`);
  }
  return value;
}

function parseApiAsset(value: unknown): GitHubReleaseAsset {
  if (!isRecord(value)) {
    throw new Error('GitHub returned an invalid release asset.');
  }

  const digestValue = value.digest;
  if (digestValue !== undefined && digestValue !== null && typeof digestValue !== 'string') {
    throw new Error('GitHub returned an invalid release asset digest.');
  }

  return {
    id: readNumber(value, 'id'),
    name: readString(value, 'name'),
    browserDownloadUrl: readString(value, 'browser_download_url'),
    size: readNumber(value, 'size'),
    contentType: readString(value, 'content_type'),
    digest: digestValue ?? null,
  };
}

function parseApiRelease(value: unknown): GitHubRelease {
  if (!isRecord(value) || !Array.isArray(value.assets)) {
    throw new Error('GitHub returned an invalid release.');
  }

  return {
    id: readNumber(value, 'id'),
    tagName: readString(value, 'tag_name'),
    name: readNullableString(value, 'name'),
    body: readNullableString(value, 'body'),
    htmlUrl: readString(value, 'html_url'),
    publishedAt: readNullableString(value, 'published_at'),
    createdAt: readString(value, 'created_at'),
    draft: readBoolean(value, 'draft'),
    prerelease: readBoolean(value, 'prerelease'),
    assets: value.assets.map(parseApiAsset),
  };
}

function parseCachedAsset(value: unknown): GitHubReleaseAsset {
  if (!isRecord(value)) {
    throw new Error('The cached release contains an invalid asset.');
  }

  const digestValue = value.digest;
  if (digestValue !== null && typeof digestValue !== 'string') {
    throw new Error('The cached release contains an invalid asset digest.');
  }

  return {
    id: readNumber(value, 'id'),
    name: readString(value, 'name'),
    browserDownloadUrl: readString(value, 'browserDownloadUrl'),
    size: readNumber(value, 'size'),
    contentType: readString(value, 'contentType'),
    digest: digestValue,
  };
}

function parseCachedRelease(value: unknown): GitHubRelease {
  if (!isRecord(value) || !Array.isArray(value.assets)) {
    throw new Error('The cached release is invalid.');
  }

  return {
    id: readNumber(value, 'id'),
    tagName: readString(value, 'tagName'),
    name: readNullableString(value, 'name'),
    body: readNullableString(value, 'body'),
    htmlUrl: readString(value, 'htmlUrl'),
    publishedAt: readNullableString(value, 'publishedAt'),
    createdAt: readString(value, 'createdAt'),
    draft: readBoolean(value, 'draft'),
    prerelease: readBoolean(value, 'prerelease'),
    assets: value.assets.map(parseCachedAsset),
  };
}

function parseChecksumCache(value: unknown): Record<string, Record<string, string>> {
  if (!isRecord(value)) {
    throw new Error('The cached checksums are invalid.');
  }

  const result: Record<string, Record<string, string>> = {};

  for (const [tagName, checksums] of Object.entries(value)) {
    if (!isRecord(checksums)) {
      throw new Error('The cached checksums are invalid.');
    }

    const parsedChecksums: Record<string, string> = {};
    for (const [filename, checksum] of Object.entries(checksums)) {
      if (typeof checksum !== 'string' || !isSha256(checksum)) {
        throw new Error('The cached checksums are invalid.');
      }
      parsedChecksums[filename] = checksum.toLowerCase();
    }
    result[tagName] = parsedChecksums;
  }

  return result;
}

export function parseGitHubReleaseResponse(value: unknown): GitHubRelease[] {
  if (!Array.isArray(value)) {
    throw new Error('GitHub returned an invalid releases response.');
  }

  return value.map(parseApiRelease);
}

export function parseCachedReleaseData(value: unknown): CachedReleaseData | null {
  try {
    if (
      !isRecord(value) ||
      value.version !== RELEASE_CACHE_VERSION ||
      typeof value.cachedAt !== 'number' ||
      !Number.isFinite(value.cachedAt) ||
      !Array.isArray(value.releases)
    ) {
      return null;
    }

    return {
      version: RELEASE_CACHE_VERSION,
      cachedAt: value.cachedAt,
      releases: value.releases.map(parseCachedRelease),
      checksumsByTag: parseChecksumCache(value.checksumsByTag),
    };
  } catch {
    return null;
  }
}

export function findLatestStableOppaRelease(releases: GitHubRelease[]): GitHubRelease | null {
  return (
    releases
      .filter(
        (release) =>
          !release.draft &&
          !release.prerelease &&
          release.tagName.toLowerCase().startsWith('oppa-v') &&
          release.assets.some(isInstallerAsset),
      )
      .sort((left, right) => releaseTimestamp(right) - releaseTimestamp(left))[0] ?? null
  );
}

export function createDownloadRelease(release: GitHubRelease, checksums: Record<string, string> = {}): DownloadRelease {
  const assetsByPlatform: Record<DownloadPlatform, DownloadAsset[]> = {
    macOS: [],
    Windows: [],
    Linux: [],
  };

  for (const asset of release.assets) {
    const details = classifyInstallerAsset(asset.name);
    if (!details) continue;

    assetsByPlatform[details.platform].push({
      ...asset,
      ...details,
      sha256: extractDigestSha256(asset.digest) ?? checksums[asset.name] ?? null,
    });
  }

  for (const platform of PLATFORM_ORDER) {
    assetsByPlatform[platform].sort((left, right) => {
      const architectureDifference =
        ARCHITECTURE_ORDER.indexOf(left.architecture) - ARCHITECTURE_ORDER.indexOf(right.architecture);
      if (architectureDifference !== 0) return architectureDifference;
      return left.format.localeCompare(right.format);
    });
  }

  return {
    release,
    assetsByPlatform,
    checksumAsset: findChecksumAsset(release.assets),
  };
}

export function findChecksumAsset(assets: GitHubReleaseAsset[]): GitHubReleaseAsset | null {
  return (
    assets.find((asset) => asset.name.toLowerCase() === 'sha256sums.txt') ??
    assets.find((asset) => /^(?:sha256sums|checksums)(?:\.[a-z0-9]+)?$/i.test(asset.name)) ??
    null
  );
}

export function parseSha256Document(value: string): Record<string, string> {
  const checksums: Record<string, string> = {};

  for (const line of value.split(/\r?\n/u)) {
    const trimmedLine = line.trim();
    if (!trimmedLine || trimmedLine.startsWith('#')) continue;

    const standardMatch = /^([a-fA-F0-9]{64})\s+[*]?(.+?)\s*$/u.exec(trimmedLine);
    if (standardMatch) {
      const [, checksum, filename] = standardMatch;
      if (checksum && filename) checksums[normalizeChecksumFilename(filename)] = checksum.toLowerCase();
      continue;
    }

    const bsdMatch = /^SHA256\s+\((.+)\)\s*=\s*([a-fA-F0-9]{64})$/iu.exec(trimmedLine);
    if (bsdMatch) {
      const [, filename, checksum] = bsdMatch;
      if (checksum && filename) checksums[normalizeChecksumFilename(filename)] = checksum.toLowerCase();
    }
  }

  return checksums;
}

export function formatReleaseNotes(body: string | null, maxLength = 240): string | null {
  if (!body) return null;

  const normalized = body
    .replace(/<!--[\s\S]*?-->/gu, ' ')
    .replace(/[#>*_`~[\]]/gu, '')
    .replace(/\((https?:\/\/[^)]+)\)/gu, '')
    .replace(/\s+/gu, ' ')
    .trim();

  if (!normalized) return null;
  if (normalized.length <= maxLength) return normalized;

  const shortened = normalized.slice(0, maxLength + 1);
  const lastSpace = shortened.lastIndexOf(' ');
  return `${shortened.slice(0, lastSpace > maxLength * 0.7 ? lastSpace : maxLength).trimEnd()}…`;
}

function classifyInstallerAsset(
  filename: string,
): Pick<DownloadAsset, 'platform' | 'architecture' | 'format' | 'architectureLabel'> | null {
  const lowerName = filename.toLowerCase();
  const architecture = detectArchitecture(lowerName);
  const architectureLabel = getArchitectureLabel(architecture);

  if (lowerName.endsWith('.dmg')) {
    return { platform: 'macOS', architecture, architectureLabel, format: 'DMG installer' };
  }
  if (lowerName.endsWith('.exe')) {
    return { platform: 'Windows', architecture, architectureLabel, format: 'NSIS installer' };
  }
  if (lowerName.endsWith('.msi')) {
    return { platform: 'Windows', architecture, architectureLabel, format: 'MSI installer' };
  }
  if (lowerName.endsWith('.appimage')) {
    return { platform: 'Linux', architecture, architectureLabel, format: 'AppImage' };
  }
  if (lowerName.endsWith('.deb')) {
    return { platform: 'Linux', architecture, architectureLabel, format: 'Debian package' };
  }
  if (lowerName.endsWith('.rpm')) {
    return { platform: 'Linux', architecture, architectureLabel, format: 'RPM package' };
  }

  return null;
}

function isInstallerAsset(asset: GitHubReleaseAsset): boolean {
  const lowerName = asset.name.toLowerCase();
  return INSTALLER_EXTENSIONS.some((extension) => lowerName.endsWith(extension));
}

function detectArchitecture(filename: string): DownloadArchitecture {
  if (/universal/iu.test(filename)) return 'universal';
  if (/(?:aarch64|arm64|arm64ec)/iu.test(filename)) return 'arm64';
  if (/(?:x86_64|x64|amd64)/iu.test(filename)) return 'x64';
  if (/(?:armv7|armhf)/iu.test(filename)) return 'armv7';
  if (/(?:i[3-6]86|x86(?!_64))/iu.test(filename)) return 'x86';
  return 'unknown';
}

function getArchitectureLabel(architecture: DownloadArchitecture): string {
  switch (architecture) {
    case 'universal':
      return 'Universal';
    case 'arm64':
      return 'ARM64';
    case 'x64':
      return 'Intel / AMD 64-bit';
    case 'armv7':
      return 'ARMv7';
    case 'x86':
      return 'Intel / AMD 32-bit';
    case 'unknown':
      return 'Architecture not specified';
  }
}

function extractDigestSha256(digest: string | null): string | null {
  if (!digest) return null;
  const match = /^sha256:([a-fA-F0-9]{64})$/u.exec(digest.trim());
  return match?.[1]?.toLowerCase() ?? null;
}

function isSha256(value: string): boolean {
  return /^[a-fA-F0-9]{64}$/u.test(value);
}

function normalizeChecksumFilename(filename: string): string {
  return filename.replace(/^\.\/+/u, '').trim();
}

function releaseTimestamp(release: GitHubRelease): number {
  const timestamp = Date.parse(release.publishedAt ?? release.createdAt);
  return Number.isFinite(timestamp) ? timestamp : 0;
}
