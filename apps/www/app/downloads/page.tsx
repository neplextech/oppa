import type { Metadata } from 'next';

import { DownloadsClient } from '@/components/downloads/downloads-client';

export const metadata: Metadata = {
  title: 'Download OPPA',
  description:
    'Download the latest stable OPPA desktop agent for macOS, Windows, or Linux, with published SHA-256 checksums.',
};

export default function DownloadsPage() {
  return <DownloadsClient />;
}
