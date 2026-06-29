import { createHighlighter, type Highlighter } from 'shiki';
import { bamlJinjaTextmate, bamlTextmate } from '@/lib/mdx/shiki-grammars';

/**
 * A single shared Shiki highlighter for the /learn2 deck.
 *
 * `BamlCode` reads this via React's `use()` (no `useEffect`), so the same
 * promise serves SSR and the client. It is memoised at module scope so the
 * (relatively expensive) WASM + grammar load happens once per process /
 * client session and is reused across every slide.
 */
let highlighterPromise: Promise<Highlighter> | null = null;

export function getLearnHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      // light + dark presets (see code-theme.tsx). Loading extras is cheap and
      // lets a page swap the BamlCode theme without re-creating the highlighter.
      themes: ['github-light', 'github-dark', 'tokyo-night'],
      langs: [
        // Custom BAML grammar (TextMate -> Shiki) reused from the docs/MDX stack.
        bamlTextmate,
        bamlJinjaTextmate,
        // Languages used by comparison / host-interop slides.
        'python',
        'typescript',
        'bash',
        'go',
        'rust',
        'json',
        'yaml',
      ],
    });
  }
  return highlighterPromise;
}
