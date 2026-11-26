import type { DownloadAsset, DownloadPlatform } from './github-releases';

/** Browser hints used to suggest an installer without requesting device details. */
export interface DownloadDevice {
  /** Browser user-agent string, including Android when exposed. */
  userAgent: string;
  /** Touch capability used to distinguish iPad desktop mode from macOS. */
  maxTouchPoints?: number;
  /** Optional low-entropy Client Hints platform, available in Chromium. */
  userAgentData?: { platform?: string };
}

/**
 * Detects the download platform, keeping Android ahead of generic Linux hints.
 * @param device Browser platform hints.
 */
export function detectDownloadPlatform(device: DownloadDevice): DownloadPlatform | null {
  const { userAgent, userAgentData, maxTouchPoints = 0 } = device;
  if (/Android/i.test(userAgentData?.platform ?? '') || /Android/i.test(userAgent)) return 'Android';
  if (/iPhone|iPad|iPod/i.test(userAgent) || (/Mac/i.test(userAgent) && maxTouchPoints > 1)) return null;
  const platform = userAgentData?.platform || userAgent;
  if (/Mac/i.test(platform)) return 'macOS';
  if (/Win/i.test(platform)) return 'Windows';
  if (/Linux/i.test(platform)) return 'Linux';
  return null;
}

/**
 * Selects a usable installer, preferring the universal APK on Android devices.
 * @param assets Installers in the selected release.
 * @param platform Detected or explicitly selected operating system.
 */
export function pickRecommendedAsset(assets: DownloadAsset[], platform: DownloadPlatform): DownloadAsset | null {
  const matching = assets.filter((asset) => asset.platform === platform);
  const preferred = platform === 'Android' ? ['universal', 'arm64'] : platform === 'macOS' ? ['arm64'] : ['x64'];
  for (const architecture of preferred) {
    const asset = matching.find((candidate) => candidate.architecture === architecture);
    if (asset) return asset;
  }
  return matching[0] ?? null;
}
