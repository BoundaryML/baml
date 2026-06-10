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
      themes: ['github-light'],
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
