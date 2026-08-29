import { docs } from 'collections/server';
import { loader } from 'fumadocs-core/source';

import { asyncapi } from './asyncapi';

export const source = loader({
  baseUrl: '/docs',
  plugins: [asyncapi.loaderPlugin()],
  source: docs.toFumadocsSource(),
});

export function getPageImageUrl(page: (typeof source)['$inferPage']) {
  const segments = [...page.slugs, 'image.png'];

  return {
    segments,
    url: `/${['og', 'docs', ...segments].join('/')}`,
  };
}
