import { llms } from 'fumadocs-core/source';

import { source } from '@/lib/source';

export function GET() {
  return new Response(llms(source).index());
}
