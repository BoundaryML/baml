'use client';

import { useEffect, useState } from 'react';
import bamlGrammar from '../../lib/baml-grammar.json';

// Map a file extension to a shiki language id. Anything unmapped renders as
// plain text (shiki's built-in no-op language), never erroring.
const EXT_LANG: Record<string, string> = {
  baml: 'baml',
  bash: 'bash',
  js: 'javascript',
  json: 'json',
  jsx: 'tsx',
  md: 'markdown',
  py: 'python',
  rs: 'rust',
  sh: 'bash',
  toml: 'toml',
  ts: 'typescript',
  tsx: 'tsx',
  txt: 'text',
  yaml: 'yaml',
  yml: 'yaml',
};

function langFor(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return EXT_LANG[ext] ?? 'text';
}

// Single shiki highlighter, built lazily on first use and reused. Dynamically
// imported so the (heavy) shiki bundle only loads when a reader actually expands
// a file. Registers the BAML TextMate grammar alongside the common languages.
let highlighterPromise: Promise<{
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
  getLoadedLanguages: () => string[];
}> | null = null;

function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = Promise.all([
      import('shiki'),
      // Pure-JS regex engine — avoids shiki's default oniguruma WASM, which
      // fails to load in the browser bundle (that silent failure leaves code
      // unhighlighted). Works reliably client-side.
      import('shiki/engine/javascript'),
    ]).then(([{ createHighlighter }, { createJavaScriptRegexEngine }]) =>
      createHighlighter({
        engine: createJavaScriptRegexEngine({ forgiving: true }),
        langs: [
          'bash',
          'javascript',
          'json',
          'markdown',
          'python',
          'rust',
          'toml',
          'tsx',
          'typescript',
          'yaml',
          // raw TextMate grammar works directly as a shiki LanguageInput
          { ...(bamlGrammar as object), aliases: [], name: 'baml' },
        ] as never,
        themes: ['github-light'],
      }),
    );
  }
  return highlighterPromise;
}

export default function CodeView({
  path,
  content,
}: {
  path: string;
  content: string;
}) {
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getHighlighter()
      .then((hl) => {
        const want = langFor(path);
        const lang = hl.getLoadedLanguages().includes(want) ? want : 'text';
        const out = hl.codeToHtml(content ?? '', {
          lang,
          theme: 'github-light',
        });
        if (alive) setHtml(out);
      })
      .catch(() => alive && setHtml(null));
    return () => {
      alive = false;
    };
  }, [path, content]);

  // Until highlighting resolves (or if it fails) show the plain pre — identical
  // markup to before, so nothing regresses.
  if (html === null) {
    return <pre className="tool-input">{content}</pre>;
  }
  // eslint-disable-next-line react/no-danger
  return <div className="code-hl" dangerouslySetInnerHTML={{ __html: html }} />;
}
