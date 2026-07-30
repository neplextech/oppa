'use client';

import { useEffect, useState } from 'react';

import {
  findChecksumAsset,
  findLatestStableOppaRelease,
  GITHUB_RELEASES_URL,
  parseCachedReleaseData,
  parseGitHubReleaseResponse,
  parseSha256Document,
  RELEASE_CACHE_KEY,
  RELEASE_CACHE_TTL_MS,
  RELEASE_CACHE_VERSION,
  RELEASE_REFRESH_BACKOFF_KEY,
  RELEASE_REFRESH_BACKOFF_MS,
  type CachedReleaseData,
  type GitHubRelease,
} from '@/lib/github-releases';

interface ReleaseState {
  cache: CachedReleaseData | null;
  loading: boolean;
  refreshing: boolean;
  usingStaleCache: boolean;
  error: string | null;
}

interface RequestError extends Error {
  retryAt?: number;
}

const MAX_CHECKSUM_FILE_SIZE = 1024 * 1024;
const RELEASE_REQUEST_TIMEOUT_MS = 10_000;
const CHECKSUM_REQUEST_TIMEOUT_MS = 5_000;

let memoryCache: CachedReleaseData | null = null;
let inFlightRequest: Promise<CachedReleaseData> | null = null;

export function useOppaRelease(): ReleaseState {
  const [state, setState] = useState<ReleaseState>({
    cache: null,
    loading: true,
    refreshing: false,
    usingStaleCache: false,
    error: null,
  });

  useEffect(() => {
    let active = true;
    const cached = readReleaseCache();
    const now = Date.now();

    if (cached && now - cached.cachedAt < RELEASE_CACHE_TTL_MS) {
      setState({
        cache: cached,
        loading: false,
        refreshing: false,
        usingStaleCache: false,
        error: null,
      });
      return () => {
        active = false;
      };
    }

    const retryAt = readRefreshBackoff();
    if (retryAt > now) {
      setState({
        cache: cached,
        loading: false,
        refreshing: false,
        usingStaleCache: cached !== null,
        error: cached
          ? 'GitHub could not be refreshed recently. Showing the last saved release.'
          : unavailableMessage(),
      });
      return () => {
        active = false;
      };
    }

    if (cached) {
      setState({
        cache: cached,
        loading: false,
        refreshing: true,
        usingStaleCache: false,
        error: null,
      });
    }

    void requestFreshReleaseData(cached)
      .then((freshCache) => {
        if (!active) return;
        setState({
          cache: freshCache,
          loading: false,
          refreshing: false,
          usingStaleCache: false,
          error: null,
        });
      })
      .catch((error: unknown) => {
        if (!active) return;
        const message = error instanceof Error ? error.message : unavailableMessage();
        setState({
          cache: cached,
          loading: false,
          refreshing: false,
          usingStaleCache: cached !== null,
          error: cached ? `${message} Showing the last saved release.` : message,
        });
      });

    return () => {
      active = false;
    };
  }, []);

  return state;
}

function readReleaseCache(): CachedReleaseData | null {
  try {
    const storedValue = window.localStorage.getItem(RELEASE_CACHE_KEY);
    const storedCache = storedValue ? parseCachedReleaseData(JSON.parse(storedValue) as unknown) : null;
    if (storedCache && (!memoryCache || storedCache.cachedAt > memoryCache.cachedAt)) {
      memoryCache = storedCache;
    }
  } catch {
    // Fall through to the in-memory cache when local storage is restricted.
  }

  return memoryCache;
}

function writeReleaseCache(cache: CachedReleaseData): void {
  memoryCache = cache;

  try {
    window.localStorage.setItem(RELEASE_CACHE_KEY, JSON.stringify(cache));
  } catch {
    // The in-memory cache still prevents duplicate requests during this visit.
  }
}

function readRefreshBackoff(): number {
  try {
    const value = window.sessionStorage.getItem(RELEASE_REFRESH_BACKOFF_KEY);
    if (!value) return 0;
    const retryAt = Number(value);
    return Number.isFinite(retryAt) ? retryAt : 0;
  } catch {
    return 0;
  }
}

function writeRefreshBackoff(retryAt: number): void {
  try {
    window.sessionStorage.setItem(RELEASE_REFRESH_BACKOFF_KEY, String(retryAt));
  } catch {
    // A storage restriction should not hide the original request result.
  }
}

function clearRefreshBackoff(): void {
  try {
    window.sessionStorage.removeItem(RELEASE_REFRESH_BACKOFF_KEY);
  } catch {
    // The successful release cache remains authoritative.
  }
}

function requestFreshReleaseData(existingCache: CachedReleaseData | null): Promise<CachedReleaseData> {
  if (inFlightRequest) return inFlightRequest;

  inFlightRequest = fetchFreshReleaseData(existingCache).finally(() => {
    inFlightRequest = null;
  });

  return inFlightRequest;
}

async function fetchFreshReleaseData(existingCache: CachedReleaseData | null): Promise<CachedReleaseData> {
  let response: Response;

  try {
    response = await fetch(GITHUB_RELEASES_URL, {
      headers: {
        Accept: 'application/vnd.github+json',
      },
      signal: AbortSignal.timeout(RELEASE_REQUEST_TIMEOUT_MS),
    });
  } catch {
    const error = requestError('The latest release could not be reached.');
    writeRefreshBackoff(error.retryAt ?? Date.now() + RELEASE_REFRESH_BACKOFF_MS);
    throw error;
  }

  if (!response.ok) {
    const rateLimitReset = Number(response.headers.get('x-ratelimit-reset'));
    const retryAt =
      response.status === 403 && Number.isFinite(rateLimitReset)
        ? Math.max(Date.now() + RELEASE_REFRESH_BACKOFF_MS, rateLimitReset * 1000)
        : Date.now() + RELEASE_REFRESH_BACKOFF_MS;
    const message =
      response.status === 403
        ? 'GitHub’s public API rate limit was reached.'
        : `GitHub returned ${response.status} while checking releases.`;
    const error = requestError(message, retryAt);
    writeRefreshBackoff(retryAt);
    throw error;
  }

  let releases: GitHubRelease[];
  try {
    releases = parseGitHubReleaseResponse((await response.json()) as unknown);
  } catch {
    const error = requestError('GitHub returned release data the page could not safely read.');
    writeRefreshBackoff(error.retryAt ?? Date.now() + RELEASE_REFRESH_BACKOFF_MS);
    throw error;
  }

  const latestRelease = findLatestStableOppaRelease(releases);
  const checksumsByTag: Record<string, Record<string, string>> = {};

  if (latestRelease) {
    const existingChecksums = existingCache?.checksumsByTag[latestRelease.tagName];
    const freshChecksums = await fetchReleaseChecksums(latestRelease);
    const checksums = freshChecksums ?? existingChecksums;
    if (checksums) checksumsByTag[latestRelease.tagName] = checksums;
  }

  const cache: CachedReleaseData = {
    version: RELEASE_CACHE_VERSION,
    cachedAt: Date.now(),
    releases,
    checksumsByTag,
  };

  writeReleaseCache(cache);
  clearRefreshBackoff();
  return cache;
}

async function fetchReleaseChecksums(release: GitHubRelease): Promise<Record<string, string> | null> {
  const checksumAsset = findChecksumAsset(release.assets);
  if (!checksumAsset || checksumAsset.size > MAX_CHECKSUM_FILE_SIZE) return null;

  try {
    const response = await fetch(checksumAsset.browserDownloadUrl, {
      signal: AbortSignal.timeout(CHECKSUM_REQUEST_TIMEOUT_MS),
    });
    if (!response.ok) return null;

    const contentLength = Number(response.headers.get('content-length'));
    if (Number.isFinite(contentLength) && contentLength > MAX_CHECKSUM_FILE_SIZE) return null;

    const body = await response.text();
    if (body.length > MAX_CHECKSUM_FILE_SIZE) return null;

    const checksums = parseSha256Document(body);
    return Object.keys(checksums).length > 0 ? checksums : null;
  } catch {
    return null;
  }
}

function requestError(message: string, retryAt = Date.now() + RELEASE_REFRESH_BACKOFF_MS): RequestError {
  const error = new Error(message) as RequestError;
  error.retryAt = retryAt;
  return error;
}

function unavailableMessage(): string {
  return 'Release information is temporarily unavailable. Visit GitHub to check for downloads.';
}
