import { generate as DefaultImage } from 'fumadocs-ui/og';
import { notFound } from 'next/navigation';
import { ImageResponse } from 'next/og';

import { OpenPrinterMark, OPEN_GRAPH_IMAGE_SIZE } from '@/components/open-graph/marketing-image';
import { getPageImageUrl, source } from '@/lib/source';

export const revalidate = false;

export async function GET(_request: Request, { params }: { params: Promise<{ slug: string[] }> }) {
  const { slug } = await params;

  if (slug.at(-1) !== 'image.png') {
    notFound();
  }

  const page = source.getPage(slug.slice(0, -1));

  if (!page) {
    notFound();
  }

  return new ImageResponse(
    <DefaultImage
      description={page.data.description}
      icon={<OpenPrinterMark color="#fb923c" />}
      primaryColor="rgba(251, 146, 60, 0.34)"
      primaryTextColor="#34d399"
      site="OpenPrinter Docs"
      title={page.data.title}
    />,
    OPEN_GRAPH_IMAGE_SIZE,
  );
}

export function generateStaticParams() {
  return source.getPages().map((page) => ({
    slug: getPageImageUrl(page).segments,
  }));
}
