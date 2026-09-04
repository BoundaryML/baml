import bamlGrammar from '@b/pkg-grammar';
import {
  type BundledLanguage,
  createHighlighter,
  type ThemedToken,
} from 'shiki';

function createDocsHighlighter() {
  return createHighlighter({
    langs: [bamlGrammar, 'toml'],
    themes: ['github-light', 'github-dark'],
  });
}

let highlighterPromise: ReturnType<typeof createDocsHighlighter> | null = null;

function getHighlighter(): ReturnType<typeof createDocsHighlighter> {
  highlighterPromise ??= createDocsHighlighter();
  return highlighterPromise;
}

function registeredLanguage(language: 'baml' | 'toml'): BundledLanguage {
  if (language === 'toml') return language;
  // SAFETY: createDocsHighlighter registers the canonical custom grammar named baml.
  return language as BundledLanguage;
}

export async function highlightCode(
  code: string,
  language: 'baml' | 'toml',
): Promise<{ dark: ThemedToken[][]; light: ThemedToken[][] }> {
  const highlighter = await getHighlighter();
  const registered = registeredLanguage(language);
  return {
    dark: highlighter.codeToTokensBase(code, {
      lang: registered,
      theme: 'github-dark',
    }),
    light: highlighter.codeToTokensBase(code, {
      lang: registered,
      theme: 'github-light',
    }),
  };
}
