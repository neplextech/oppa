import { CopyButton } from '@/components/copy-button';
import { type CodeLang, highlightCode } from '@/lib/shiki';

/**
 * Server-rendered, Shiki-highlighted code panel. Highlighting runs
 * ahead of time on the server so the client receives static markup —
 * no hydration pass re-tokenizes the code, so there is nothing to
 * flash unstyled.
 */
export async function CodeBlock({
  code,
  language,
  filename,
  highlightedLines = [],
  showLineNumbers = true,
}: {
  code: string;
  language: CodeLang;
  filename?: string;
  highlightedLines?: number[];
  showLineNumbers?: boolean;
}) {
  const html = await highlightCode(code, language, highlightedLines);

  return (
    <div className="overflow-hidden rounded border border-white/10 bg-[#0b0b0a]">
      {filename ? (
        <div className="flex items-center justify-between border-b border-white/10 px-4 py-2.5">
          <span className="font-mono text-[11.5px] text-stone-500">{filename}</span>
          <CopyButton code={code} />
        </div>
      ) : (
        <div className="flex items-center justify-end px-3 py-1.5">
          <CopyButton code={code} />
        </div>
      )}
      <div
        className="code-block overflow-x-auto py-3 text-[12.5px] leading-[1.65]"
        data-line-numbers={showLineNumbers ? 'true' : undefined}
        // Shiki output is generated server-side from fixed source strings in
        // this codebase, not user input.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
