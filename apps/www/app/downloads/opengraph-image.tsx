import { ImageResponse } from 'next/og';

import { MarketingOpenGraphImage, OPEN_GRAPH_IMAGE_SIZE } from '@/components/open-graph/marketing-image';

export const alt = 'Download the OPPA desktop agent for macOS, Windows, and Linux';
export const size = OPEN_GRAPH_IMAGE_SIZE;
export const contentType = 'image/png';
export const dynamic = 'force-static';

export default function Image() {
  return new ImageResponse(<MarketingOpenGraphImage variant="downloads" />, size);
}
