import type { MetadataRoute } from 'next';

import { source } from '@/lib/source';

const BASE_URL = 'https://oppa.neplex.dev';

export default function sitemap(): MetadataRoute.Sitemap {
  const pages = source.getPages();

  const docPages: MetadataRoute.Sitemap = pages.map((page) => ({
    url: `${BASE_URL}${page.url}`,
    changeFrequency: 'weekly',
    priority: 0.5,
  }));

  return [
    {
      url: BASE_URL,
      changeFrequency: 'monthly',
      priority: 1,
    },
    {
      url: `${BASE_URL}/docs`,
      changeFrequency: 'monthly',
      priority: 0.8,
    },
    {
      url: `${BASE_URL}/downloads`,
      changeFrequency: 'weekly',
      priority: 0.7,
    },
    ...docPages,
  ];
}
