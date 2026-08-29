import { DocsBody, DocsDescription, DocsPage, DocsTitle } from 'fumadocs-ui/layouts/docs/page';
import type { Metadata } from 'next';
import { notFound } from 'next/navigation';

import { AsyncAPIPage } from '@/components/asyncapi-page';
import { getMDXComponents } from '@/components/mdx';
import { asyncapi } from '@/lib/asyncapi';
import { getPageImageUrl, source } from '@/lib/source';

export default async function Page({ params }: { params: Promise<{ slug?: string[] }> }) {
  const { slug } = await params;
  const page = source.getPage(slug);

  if (!page) {
    notFound();
  }

  const Content = page.data.body;
  const asyncApiProps = await asyncapi.preloadAsyncAPIPage(page);
  const isGeneratedAsyncApiPage = page.slugs[0] === 'agent-gateway' && page.slugs.length > 1;
  const preloadedAsyncApiProps =
    isGeneratedAsyncApiPage && asyncApiProps.preloaded.docs['openprinter-cloud'] === undefined
      ? {
          preloaded: {
            ...asyncApiProps.preloaded,
            docs: {
              ...asyncApiProps.preloaded.docs,
              'openprinter-cloud': (await asyncapi.getSchema('openprinter-cloud')).bundled,
            },
          },
        }
      : asyncApiProps;

  return (
    <DocsPage toc={page.data.toc} full={page.data.full}>
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription>{page.data.description}</DocsDescription>
      <DocsBody>
        <Content
          components={getMDXComponents({
            AsyncAPIPage: (props) => <AsyncAPIPage {...preloadedAsyncApiProps} {...props} />,
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata({ params }: { params: Promise<{ slug?: string[] }> }): Promise<Metadata> {
  const { slug } = await params;
  const page = source.getPage(slug);

  if (!page) {
    notFound();
  }

  const image = getPageImageUrl(page);

  return {
    title: page.data.title,
    description: page.data.description,
    openGraph: {
      description: page.data.description,
      images: [
        {
          alt: `${page.data.title} — OpenPrinter Docs`,
          height: 630,
          url: image.url,
          width: 1200,
        },
      ],
      title: page.data.title,
      type: 'article',
    },
    twitter: {
      card: 'summary_large_image',
      description: page.data.description,
      images: [image.url],
      title: page.data.title,
    },
  };
}
