'use client';

import {
  ArrowDownToLine,
  ArrowUpRight,
  Box,
  Check,
  CircleAlert,
  Github,
  HardDriveDownload,
  LoaderCircle,
  PackageOpen,
  RefreshCw,
  ShieldCheck,
} from 'lucide-react';
import Link from 'next/link';
import type { ComponentType, SVGProps } from 'react';
import { useMemo } from 'react';

import { SiteHeader } from '@/components/site-header';
import {
  createDownloadRelease,
  findLatestStableOppaRelease,
  formatReleaseNotes,
  GITHUB_RELEASES_PAGE_URL,
  type DownloadAsset,
  type DownloadPlatform,
} from '@/lib/github-releases';

import { LinuxPlatformIcon, MacPlatformIcon, WindowsPlatformIcon } from './platform-icons';
import { useOppaRelease } from './use-oppa-release';

function detectPlatform(): DownloadPlatform | null {
  if (typeof navigator === 'undefined') return null;
  const ua = navigator.userAgent;
  if (/Mac/i.test(ua)) return 'macOS';
  if (/Win/i.test(ua)) return 'Windows';
  if (/Linux/i.test(ua)) return 'Linux';
  return null;
}

function pickBestAsset(assets: DownloadAsset[], platform: DownloadPlatform): DownloadAsset | null {
  const platformAssets = assets.filter((a) => a.platform === platform);
  if (platformAssets.length === 0) return null;
  // Prefer arm64 on macOS (Apple Silicon), x64 everywhere else
  const preferred = platform === 'macOS' ? 'arm64' : 'x64';
  return platformAssets.find((a) => a.architecture === preferred) ?? platformAssets[0];
}

function platformLabel(platform: DownloadPlatform): string {
  if (platform === 'macOS') return 'Download for Mac';
  if (platform === 'Windows') return 'Download for Windows';
  return 'Download for Linux';
}

interface PlatformDetails {
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  eyebrow: string;
  description: string;
  accent: string;
}

const PLATFORM_DETAILS: Record<DownloadPlatform, PlatformDetails> = {
  macOS: {
    icon: MacPlatformIcon,
    eyebrow: 'Apple silicon and Intel',
    description: 'Install with a disk image for Apple silicon or Intel.',
    accent: 'text-orange-300 bg-orange-400/10 border-orange-400/20',
  },
  Windows: {
    icon: WindowsPlatformIcon,
    eyebrow: 'x64 and ARM64 builds',
    description: 'Choose the standard installer or an MSI package for managed systems.',
    accent: 'text-sky-300 bg-sky-400/10 border-sky-400/20',
  },
  Linux: {
    icon: LinuxPlatformIcon,
    eyebrow: 'Desktop Linux',
    description: 'Use AppImage or your distribution’s native package format.',
    accent: 'text-emerald-300 bg-emerald-400/10 border-emerald-400/20',
  },
};

const PLATFORMS = Object.keys(PLATFORM_DETAILS) as DownloadPlatform[];

export function DownloadsClient() {
  const { cache, loading, refreshing, usingStaleCache, error } = useOppaRelease();
  const latestRelease = cache ? findLatestStableOppaRelease(cache.releases) : null;
  const downloadRelease = latestRelease
    ? createDownloadRelease(latestRelease, cache?.checksumsByTag[latestRelease.tagName])
    : null;
  const releaseNotes = formatReleaseNotes(latestRelease?.body ?? null);

  const detectedPlatform = useMemo(() => detectPlatform(), []);
  const allAssets = downloadRelease
    ? (['macOS', 'Windows', 'Linux'] as DownloadPlatform[]).flatMap((p) => downloadRelease.assetsByPlatform[p])
    : [];
  const suggestedAsset = detectedPlatform && !loading ? pickBestAsset(allAssets, detectedPlatform) : null;
  const suggestedPlatformDetails = detectedPlatform ? PLATFORM_DETAILS[detectedPlatform] : null;
  const SuggestedPlatformIcon = suggestedPlatformDetails?.icon ?? ArrowDownToLine;
  const suggestedPlatformLabel = detectedPlatform ? platformLabel(detectedPlatform) : 'Download';

  return (
    <main className="relative min-h-screen overflow-hidden bg-[#0a0a09] text-stone-100">
      <SiteHeader />

      <section className="relative border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 pt-16 pb-14 lg:px-8 lg:pt-20 lg:pb-16">
          <div className="grid gap-12 lg:grid-cols-[1fr_0.72fr] lg:items-end">
            <div>
              <div className="flex items-center gap-2 font-mono text-[11px] tracking-[0.14em] text-stone-500 uppercase">
                <span className="size-1.5 rounded-full bg-emerald-400" />
                OPPA desktop / stable channel
              </div>
              <h1 className="mt-5 max-w-2xl text-[2.65rem] leading-[1.03] font-semibold tracking-[-0.035em] text-balance sm:text-[3.4rem]">
                Your local printers,
                <br />
                <span className="text-orange-400">within reach.</span>
              </h1>
              <p className="mt-6 max-w-xl text-[14px] leading-6 text-stone-400 sm:text-[15px]">
                Download the Open Printer Proxy Agent for the machine connected to your printers. OPPA runs locally,
                keeps credentials in operating-system secure storage, and accepts only the documented OpenPrinter
                protocol.
              </p>
              <div className="mt-8 flex w-full max-w-sm flex-col gap-3">
                {suggestedAsset ? (
                  <a
                    className="group inline-flex min-h-12 w-full items-center gap-3 rounded border border-orange-400/30 bg-stone-950 px-4 py-2 text-left text-[13px] font-medium text-stone-50 shadow-[inset_0_0_0_1px_rgba(249,115,22,0.08)] transition hover:border-orange-400/45 hover:bg-stone-900"
                    href={suggestedAsset.browserDownloadUrl}
                    title={`${suggestedAsset.format} · ${suggestedAsset.architectureLabel}`}
                  >
                    <div
                      className={`flex size-8 shrink-0 items-center justify-center rounded-md border ${suggestedPlatformDetails?.accent ?? 'border-white/10 bg-white/5 text-stone-200'}`}
                    >
                      <SuggestedPlatformIcon className="size-4" aria-hidden />
                    </div>
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate">{suggestedPlatformLabel}</span>
                      <span className="truncate font-mono text-[10px] tracking-[0.06em] text-stone-300 uppercase">
                        {suggestedAsset.architectureLabel} · {formatFileSize(suggestedAsset.size)} ·{' '}
                        {suggestedAsset.format}
                      </span>
                    </span>
                    <ArrowDownToLine
                      className="size-3.5 shrink-0 text-stone-400 transition group-hover:text-stone-100"
                      aria-hidden
                    />
                  </a>
                ) : (
                  <a
                    className="inline-flex min-h-12 w-full items-center gap-1.5 rounded bg-stone-100 px-4 py-2 text-[13px] font-medium text-stone-900 transition hover:bg-white"
                    href="#downloads"
                  >
                    {loading ? 'Loading…' : 'Choose a download'}
                    <ArrowDownToLine className="size-3.5" aria-hidden />
                  </a>
                )}
                <Link
                  className="inline-flex min-h-12 w-full items-center justify-center gap-1.5 rounded border border-white/10 px-4 py-2 text-[13px] font-medium text-stone-300 transition hover:border-white/20 hover:text-stone-100"
                  href="/docs/getting-started"
                >
                  Setup guide
                  <ArrowUpRight className="size-3.5" aria-hidden />
                </Link>
              </div>
            </div>

            <ReleaseSummary
              loading={loading}
              refreshing={refreshing}
              unavailable={!loading && cache === null && error !== null}
              tagName={latestRelease?.tagName ?? null}
              publishedAt={latestRelease?.publishedAt ?? null}
              releaseName={latestRelease?.name ?? null}
            />
          </div>
        </div>
      </section>

      <section id="downloads" className="relative scroll-mt-14 border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 py-16 lg:px-8 lg:py-20">
          <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
            <div>
              <div className="font-mono text-[11px] tracking-[0.13em] text-stone-600 uppercase">Native installers</div>
              <h2 className="mt-3 text-2xl font-semibold tracking-[-0.02em]">Choose your platform.</h2>
            </div>
            {latestRelease && (
              <a
                className="inline-flex items-center gap-1.5 self-start font-mono text-[11px] text-stone-500 transition hover:text-stone-200 sm:self-auto"
                href={latestRelease.htmlUrl}
                rel="noreferrer"
                target="_blank"
              >
                Release notes
                <ArrowUpRight className="size-3" aria-hidden />
              </a>
            )}
          </div>

          {usingStaleCache && error && <CacheWarning message={error} />}

          {loading ? (
            <DownloadSkeleton />
          ) : downloadRelease ? (
            <>
              <div className="mt-9 grid gap-4 lg:grid-cols-3">
                {PLATFORMS.map((platform) => (
                  <PlatformCard
                    key={platform}
                    platform={platform}
                    assets={downloadRelease.assetsByPlatform[platform]}
                  />
                ))}
              </div>

              {releaseNotes && (
                <div className="mt-6 flex gap-3 border-l border-orange-400/30 py-1 pl-4">
                  <PackageOpen className="mt-0.5 size-4 shrink-0 text-orange-400/70" aria-hidden />
                  <p className="max-w-3xl text-[12.5px] leading-5 text-stone-500">
                    <span className="font-medium text-stone-300">From this release:</span> {releaseNotes}
                  </p>
                </div>
              )}

              <ChecksumSection
                assets={PLATFORMS.flatMap((platform) => downloadRelease.assetsByPlatform[platform])}
                checksumUrl={downloadRelease.checksumAsset?.browserDownloadUrl ?? null}
                tagName={downloadRelease.release.tagName}
              />
            </>
          ) : (
            <NoRelease error={error} />
          )}
        </div>
      </section>

      <section className="relative border-b border-white/10 bg-white/[0.015]">
        <div className="mx-auto grid max-w-6xl gap-10 px-6 py-16 md:grid-cols-3 lg:px-8">
          <TrustItem
            icon={ShieldCheck}
            title="Narrow by design"
            description="No arbitrary shell commands, scripts, or generic proxy access."
          />
          <TrustItem
            icon={HardDriveDownload}
            title="Local and recoverable"
            description="Jobs are persisted locally before acknowledgement and recover after restarts."
          />
          <TrustItem
            icon={Check}
            title="Verifiable downloads"
            description="Every installer is paired with a SHA-256 digest published in the GitHub release."
          />
        </div>
      </section>

      <footer className="relative py-8">
        <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-4 px-6 font-mono text-[11px] text-stone-500 sm:flex-row lg:px-8">
          <span>Release data is refreshed from GitHub at most once every 15 minutes.</span>
          <div className="flex gap-5">
            <Link className="transition hover:text-stone-300" href="/docs/build-oppa">
              Build from source
            </Link>
            <a
              className="transition hover:text-stone-300"
              href={GITHUB_RELEASES_PAGE_URL}
              rel="noreferrer"
              target="_blank"
            >
              All releases
            </a>
          </div>
        </div>
      </footer>
    </main>
  );
}

function ReleaseSummary({
  loading,
  refreshing,
  unavailable,
  tagName,
  publishedAt,
  releaseName,
}: {
  loading: boolean;
  refreshing: boolean;
  unavailable: boolean;
  tagName: string | null;
  publishedAt: string | null;
  releaseName: string | null;
}) {
  return (
    <aside className="relative overflow-hidden rounded-lg border border-white/10 bg-black/25 p-5">
      <div className="flex items-center justify-between gap-4">
        <span className="font-mono text-[10px] tracking-[0.14em] text-stone-600 uppercase">Latest release</span>
        {refreshing ? (
          <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-stone-500">
            <LoaderCircle className="size-3 animate-spin" aria-hidden />
            Refreshing
          </span>
        ) : unavailable ? (
          <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-amber-400/80">
            <span className="size-1.5 rounded-full bg-amber-400" />
            Unavailable
          </span>
        ) : (
          <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-emerald-400/80">
            <span className="size-1.5 rounded-full bg-emerald-400" />
            Stable channel
          </span>
        )}
      </div>

      {loading ? (
        <div className="mt-5 animate-pulse">
          <div className="h-7 w-28 rounded bg-white/[0.06]" />
          <div className="mt-3 h-3 w-40 rounded bg-white/[0.04]" />
        </div>
      ) : tagName ? (
        <>
          <div className="mt-5 flex items-baseline gap-3">
            <span className="text-2xl font-semibold tracking-[-0.03em] text-stone-100">{formatVersion(tagName)}</span>
            <span className="font-mono text-[10px] text-stone-600">{releaseName ?? 'OPPA desktop'}</span>
          </div>
          <p className="mt-2 font-mono text-[10px] text-stone-500">Published {formatReleaseDate(publishedAt)}</p>
        </>
      ) : unavailable ? (
        <>
          <p className="mt-5 text-[15px] font-medium text-stone-200">Release status unavailable.</p>
          <p className="mt-2 text-[12px] leading-5 text-stone-500">
            GitHub could not be reached and this browser has no saved release data yet.
          </p>
        </>
      ) : (
        <>
          <p className="mt-5 text-[15px] font-medium text-stone-200">The first stable build is on its way.</p>
          <p className="mt-2 text-[12px] leading-5 text-stone-500">
            This card will update automatically when an OPPA release is published.
          </p>
        </>
      )}
    </aside>
  );
}

function PlatformCard({ platform, assets }: { platform: DownloadPlatform; assets: DownloadAsset[] }) {
  const details = PLATFORM_DETAILS[platform];
  const Icon = details.icon;

  return (
    <article className="group relative flex min-h-80 flex-col overflow-hidden rounded-lg border border-white/10 bg-white/[0.025] p-5 transition duration-300 hover:border-white/20 hover:bg-white/[0.035]">
      <div className="relative flex items-start gap-3.5">
        <div className={`flex size-10 shrink-0 items-center justify-center rounded-md border ${details.accent}`}>
          <Icon className="size-[18px]" aria-hidden />
        </div>
        <div>
          <h3 className="text-[17px] font-semibold tracking-[-0.015em] text-stone-100">{platform}</h3>
          <p className="mt-0.5 font-mono text-[9.5px] tracking-wide text-stone-600 uppercase">{details.eyebrow}</p>
        </div>
      </div>
      <p className="relative mt-5 min-h-10 text-[12.5px] leading-5 text-stone-500">{details.description}</p>

      <div className="relative mt-5 flex flex-1 flex-col gap-2 border-t border-white/10 pt-4">
        {assets.length > 0 ? (
          assets.map((asset) => <DownloadButton key={asset.id} asset={asset} />)
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center rounded-md border border-dashed border-white/10 px-4 py-7 text-center">
            <Box className="size-5 text-stone-700" aria-hidden />
            <p className="mt-2 text-[11px] text-stone-600">No {platform} installer in this release.</p>
          </div>
        )}
      </div>
    </article>
  );
}

function DownloadButton({ asset }: { asset: DownloadAsset }) {
  return (
    <a
      aria-label={`Download ${asset.name}`}
      className="group/download flex items-center justify-between gap-3 rounded-md border border-white/[0.07] bg-black/25 px-3 py-3 transition hover:border-orange-400/25 hover:bg-orange-400/[0.055]"
      href={asset.browserDownloadUrl}
    >
      <span className="min-w-0">
        <span className="block truncate text-[12px] font-medium text-stone-200 group-hover/download:text-white">
          {asset.format}
        </span>
        <span className="mt-1 flex items-center gap-2 font-mono text-[9.5px] text-stone-600">
          <span>{asset.architectureLabel}</span>
          <span aria-hidden>·</span>
          <span>{formatFileSize(asset.size)}</span>
        </span>
      </span>
      <ArrowDownToLine
        className="size-3.5 shrink-0 text-stone-600 transition group-hover/download:translate-y-0.5 group-hover/download:text-orange-400"
        aria-hidden
      />
    </a>
  );
}

function ChecksumSection({
  assets,
  checksumUrl,
  tagName,
}: {
  assets: DownloadAsset[];
  checksumUrl: string | null;
  tagName: string;
}) {
  if (assets.length === 0) return null;

  return (
    <section className="mt-16 overflow-hidden rounded-lg border border-white/10 bg-black/30">
      <div className="flex flex-col justify-between gap-4 border-b border-white/10 px-5 py-5 sm:flex-row sm:items-center">
        <div>
          <div className="flex items-center gap-2">
            <ShieldCheck className="size-4 text-emerald-400" aria-hidden />
            <h3 className="text-[14px] font-medium text-stone-200">SHA-256 checksums</h3>
          </div>
          <p className="mt-1.5 text-[11px] leading-5 text-stone-500">
            Compare the digest after downloading to verify that the installer arrived unchanged.
          </p>
        </div>
        {checksumUrl && (
          <a
            className="inline-flex shrink-0 items-center gap-1.5 self-start rounded border border-white/10 px-3 py-2 font-mono text-[10px] text-stone-400 transition hover:border-white/20 hover:text-stone-100 sm:self-auto"
            href={checksumUrl}
          >
            SHA256SUMS.txt
            <ArrowDownToLine className="size-3" aria-hidden />
          </a>
        )}
      </div>

      <div className="overflow-x-auto p-2 sm:p-3">
        <table className="w-full min-w-[700px] border-separate border-spacing-0 font-mono text-[10px]">
          <caption className="sr-only">SHA-256 checksums for OPPA {tagName} installers</caption>
          <thead>
            <tr className="text-left text-stone-600">
              <th className="px-3 py-2 font-normal">File</th>
              <th className="px-3 py-2 font-normal">SHA-256</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) => (
              <tr key={asset.id} className="group/checksum">
                <td className="max-w-72 border-t border-white/[0.06] px-3 py-3 text-stone-400">
                  <a
                    className="block truncate transition group-hover/checksum:text-stone-200 hover:text-orange-400!"
                    href={asset.browserDownloadUrl}
                    title={asset.name}
                  >
                    {asset.name}
                  </a>
                </td>
                <td className="border-t border-white/[0.06] px-3 py-3">
                  {asset.sha256 ? (
                    <span className="text-stone-500">{asset.sha256}</span>
                  ) : (
                    <span className="text-amber-400/70">Digest unavailable — see the release asset</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function CacheWarning({ message }: { message: string }) {
  return (
    <div className="mt-7 flex items-start gap-3 rounded-md border border-amber-400/15 bg-amber-400/[0.045] px-4 py-3">
      <RefreshCw className="mt-0.5 size-3.5 shrink-0 text-amber-400/80" aria-hidden />
      <p className="text-[11px] leading-5 text-amber-100/60">{message}</p>
    </div>
  );
}

function NoRelease({ error }: { error: string | null }) {
  return (
    <div className="mt-9 grid min-h-72 place-items-center rounded-lg border border-dashed border-white/10 bg-white/[0.015] px-6 py-12 text-center">
      <div className="max-w-md">
        <div className="mx-auto flex size-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03]">
          {error ? (
            <CircleAlert className="size-[18px] text-amber-400" aria-hidden />
          ) : (
            <PackageOpen className="size-[18px] text-stone-500" aria-hidden />
          )}
        </div>
        <h3 className="mt-4 text-[16px] font-medium text-stone-200">
          {error ? 'Release information is unavailable' : 'No stable OPPA release yet'}
        </h3>
        <p className="mt-2 text-[12.5px] leading-5 text-stone-500">
          {error ??
            'The downloads will appear here as soon as the first stable desktop release is published. You can build OPPA from source in the meantime.'}
        </p>
        <div className="mt-6 flex flex-wrap justify-center gap-3">
          <a
            className="inline-flex h-9 items-center gap-1.5 rounded border border-white/10 px-4 text-[12px] font-medium text-stone-300 transition hover:border-white/20 hover:text-white"
            href={GITHUB_RELEASES_PAGE_URL}
            rel="noreferrer"
            target="_blank"
          >
            <Github className="size-3.5" aria-hidden />
            Check GitHub
          </a>
          <Link
            className="inline-flex h-9 items-center gap-1.5 rounded bg-stone-100 px-4 text-[12px] font-medium text-stone-900 transition hover:bg-white"
            href="/docs/build-oppa"
          >
            Build from source
            <ArrowUpRight className="size-3.5" aria-hidden />
          </Link>
        </div>
      </div>
    </div>
  );
}

function DownloadSkeleton() {
  return (
    <div className="mt-9 grid gap-4 lg:grid-cols-3" aria-label="Loading the latest release">
      {PLATFORMS.map((platform) => (
        <div key={platform} className="min-h-80 animate-pulse rounded-lg border border-white/10 bg-white/[0.02] p-5">
          <div className="flex items-center gap-3.5">
            <div className="size-10 rounded-md bg-white/[0.06]" />
            <div>
              <div className="h-4 w-20 rounded bg-white/[0.06]" />
              <div className="mt-2 h-2 w-28 rounded bg-white/[0.04]" />
            </div>
          </div>
          <div className="mt-6 h-3 w-full rounded bg-white/[0.04]" />
          <div className="mt-2 h-3 w-2/3 rounded bg-white/[0.04]" />
          <div className="mt-6 border-t border-white/10 pt-4">
            <div className="h-14 rounded-md bg-white/[0.04]" />
            <div className="mt-2 h-14 rounded-md bg-white/[0.035]" />
          </div>
        </div>
      ))}
    </div>
  );
}

function TrustItem({
  icon: Icon,
  title,
  description,
}: {
  icon: ComponentType<SVGProps<SVGSVGElement>>;
  title: string;
  description: string;
}) {
  return (
    <div className="flex gap-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-stone-500" aria-hidden />
      <div>
        <h3 className="text-[13px] font-medium text-stone-200">{title}</h3>
        <p className="mt-1.5 text-[12px] leading-5 text-stone-500">{description}</p>
      </div>
    </div>
  );
}

function formatVersion(tagName: string): string {
  return tagName.replace(/^oppa-/iu, '');
}

function formatReleaseDate(value: string | null): string {
  if (!value) return 'date unavailable';
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return 'date unavailable';
  return new Intl.DateTimeFormat('en', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  }).format(date);
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;

  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unitIndex = 0;

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}
