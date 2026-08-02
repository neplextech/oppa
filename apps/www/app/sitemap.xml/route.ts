import { source } from '@/lib/source';

export const dynamic = 'force-static';

const BASE_URL = 'https://oppa.neplex.dev';

interface SitemapEntry {
  url: string;
  changeFrequency: string;
  priority: number;
}

function toXml(entries: SitemapEntry[]): string {
  const urls = entries
    .map(
      (e) => `  <url>
    <loc>${e.url}</loc>
    <changefreq>${e.changeFrequency}</changefreq>
    <priority>${e.priority}</priority>
  </url>`,
    )
    .join('\n');

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>`;
}

export function GET(): Response {
  const pages = source.getPages();

  const entries: SitemapEntry[] = [
    { url: BASE_URL, changeFrequency: 'monthly', priority: 1 },
    { url: `${BASE_URL}/docs`, changeFrequency: 'monthly', priority: 0.8 },
    { url: `${BASE_URL}/downloads`, changeFrequency: 'weekly', priority: 0.7 },
    ...pages.map((page) => ({
      url: `${BASE_URL}${page.url}`,
      changeFrequency: 'weekly',
      priority: 0.5,
    })),
  ];

  return new Response(toXml(entries), {
    headers: { 'Content-Type': 'application/xml' },
  });
}
