'use client';

import { useEffect, useState } from 'react';

const FALLBACK = (code: string) =>
  `<pre><code>${code
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')}</code></pre>`;

interface MinimalCodeSnippetProps {
  code: string;
  lang?: 'baml' | 'python' | 'typescript';
}

export function MinimalCodeSnippet({
  code,
  lang = 'baml',
}: MinimalCodeSnippetProps) {
  const [html, setHtml] = useState<string>(() => FALLBACK(code));

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { createHighlighter } = await import('shiki');
        const grammars = await import('@/lib/mdx/shiki-grammars');
        const langs: Parameters<typeof createHighlighter>[0]['langs'] =
          lang === 'baml'
            ? [grammars.bamlTextmate, grammars.bamlJinjaTextmate]
            : [lang];
        const highlighter = await createHighlighter({
          themes: ['github-light'],
          langs,
        });
        const out = highlighter.codeToHtml(code, {
          lang,
          theme: 'github-light',
        });
        if (!cancelled) setHtml(out);
      } catch {
        /* keep fallback */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  return (
    <div
      className="minimal-code"
      style={{
        background: '#EFECE3',
        borderRadius: 6,
        padding: '20px 24px',
        fontFamily:
          '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        fontSize: 13.5,
        lineHeight: 1.6,
        overflow: 'auto',
      }}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: shiki output is sanitized HTML
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
