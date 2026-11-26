import type { Metadata } from 'next';

import { DownloadsClient } from '@/components/downloads/downloads-client';

export const metadata: Metadata = {
  title: 'Download OPPA',
  description:
    'Download the latest stable OPPA agent for macOS, Windows, Linux, or Android, with published SHA-256 checksums.',
  openGraph: {
    description:
      'Download the latest stable OPPA agent for macOS, Windows, Linux, or Android, with published SHA-256 checksums.',
    title: 'Download OPPA',
    type: 'website',
  },
};

export default function DownloadsPage() {
  return <DownloadsClient />;
}
