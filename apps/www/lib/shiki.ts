import { createHighlighter, type Highlighter } from 'shiki';
import { bundledThemes } from 'shiki/themes';

/**
 * Code block background — kept in one place so the theme and the
 * component chrome around it never drift apart.
 */
export const CODE_BG = '#0b0b0a';

const ACCENT = '#f0a568';

let highlighterPromise: Promise<Highlighter> | null = null;

async function buildTheme() {
  const vesper = (await bundledThemes.vesper()).default;
  const theme = structuredClone(vesper) as typeof vesper & {
    name: string;
    colors: Record<string, string>;
    tokenColors: { settings?: { foreground?: string } }[];
  };
  theme.name = 'oppa-vesper';
  theme.colors.background = CODE_BG;
  theme.colors.foreground = '#e7e5e4';
  for (const token of theme.tokenColors) {
    if (token.settings?.foreground === '#FFC799') {
      token.settings.foreground = ACCENT;
    }
  }
  return theme;
}

async function getHighlighter() {
  highlighterPromise ??= buildTheme().then((theme) =>
    createHighlighter({
      themes: [theme],
      langs: ['typescript', 'tsx', 'json', 'jsonc', 'bash'],
    }),
  );
  return highlighterPromise;
}

export type CodeLang = 'typescript' | 'tsx' | 'json' | 'jsonc' | 'bash';

/**
 * Server-only: renders code to Shiki's static HTML output ahead of
 * time, so the client never re-highlights or flashes unstyled text.
 */
export async function highlightCode(code: string, lang: CodeLang, highlightedLines: number[] = []): Promise<string> {
  const highlighter = await getHighlighter();
  return highlighter.codeToHtml(code, {
    lang,
    theme: 'oppa-vesper',
    decorations: highlightedLines.map((line) => ({
      start: { line: line - 1, character: 0 },
      end: { line, character: 0 },
      properties: { class: 'highlighted-line' },
    })),
  });
}
