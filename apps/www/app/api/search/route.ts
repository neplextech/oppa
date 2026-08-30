import { createFromSource } from 'fumadocs-core/search/server';

import { source } from '@/lib/source';

export const { staticGET: GET } = createFromSource(source, {
  language: 'english',
});
