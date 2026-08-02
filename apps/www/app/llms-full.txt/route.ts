import { getLLMText } from '@/lib/get-llm-text';
import { source } from '@/lib/source';

export const dynamic = 'force-static';

export async function GET() {
  const pages = source.getPages();
  const texts = await Promise.all(pages.map(getLLMText));
  return new Response(texts.join('\n\n---\n\n'));
}
