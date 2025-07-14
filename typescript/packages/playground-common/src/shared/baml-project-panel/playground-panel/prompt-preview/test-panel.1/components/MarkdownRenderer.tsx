/* eslint-disable @typescript-eslint/no-floating-promises */
'use client';

import { cn } from '@baml/ui/lib/utils';
import { compile, run } from '@mdx-js/mdx';
import type { Element } from 'hast';
import { hasProperty } from 'hast-util-has-property';
import { useTheme } from 'next-themes';
import { Fragment, useEffect, useState } from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import * as runtime from 'react/jsx-runtime';
import rehypeStringify from 'rehype-stringify';
import { visit } from 'unist-util-visit';

const rehypeTargetBlank = () => {
  return (tree: any) => {
    visit(tree, 'element', (node: Element) => {
      if (node.tagName !== 'a') return;

      if (hasProperty(node, 'target')) {
        return;
      }

      const href = node.properties?.href?.toString();
      if (
        href &&
        (href.startsWith('http://') || href.startsWith('https://')) &&
        node.properties
      ) {
        node.properties.target = '_blank';
        node.properties.rel = 'noopener noreferrer';
      }
    });
  };
};

function ErrorFallback({ error }: { error: Error }) {
  return (
    <div className="p-4 text-red-500">
      <p>Something went wrong rendering the markdown:</p>
      <pre className="mt-2 text-sm">{error.message}</pre>
    </div>
  );
}

export function MarkdownRenderer({ source }: { source: string }) {
  return (
    <ErrorBoundary FallbackComponent={ErrorFallback}>
      <MarkdownRendererContent source={source} />
    </ErrorBoundary>
  );
}

function MarkdownRendererContent({ source }: { source: string }) {
  const [mdxModule, setMdxModule] = useState<any | undefined>(undefined);
  const Content = mdxModule ? mdxModule.default : Fragment;
  const [highlighter, setHighlighter] = useState<any | undefined>(undefined);

  useEffect(() => {
    if (highlighter) return;
    (async () => {
      try {
        const { createHighlighterCore } = await import('shiki/core');
        const { createJavaScriptRegexEngine } = await import(
          'shiki/engine/javascript'
        );
        const highlighter = await createHighlighterCore({
          themes: [
            import('shiki/themes/github-dark-default.mjs'),
            import('shiki/themes/github-light.mjs'),
          ],
          langs: [
            import('shiki/langs/python.mjs'),
            import('shiki/langs/typescript.mjs'),
            import('shiki/langs/ruby.mjs'),
            import('shiki/langs/json.mjs'),
            import('shiki/langs/yaml.mjs'),
            import('shiki/langs/markdown.mjs'),
            import('shiki/langs/javascript.mjs'),
            import('shiki/langs/html.mjs'),
            import('shiki/langs/css.mjs'),
            import('shiki/langs/sql.mjs'),
            import('shiki/langs/tsx.mjs'),
            import('shiki/langs/jsx.mjs'),
            import('shiki/langs/bash.mjs'),
          ],
          engine: createJavaScriptRegexEngine(),
        });
        setHighlighter(highlighter);
      } catch (error) {
        console.error('Error creating highlighter:', error);
      }
    })();
  }, []);
  const { theme } = useTheme();

  useEffect(() => {
    if (!highlighter) return;
    (async () => {
      try {
        const rehypeShikiFromHighlighter = (
          await import('@shikijs/rehype/core')
        ).default;

        const code = await compile(source, {
          outputFormat: 'function-body',
          rehypePlugins: [
            [
              rehypeShikiFromHighlighter,
              highlighter,
              {
                themes: {
                  light:
                    theme === 'dark' ? 'github-dark-default' : 'github-light',
                  dark:
                    theme === 'dark' ? 'github-dark-default' : 'github-light',
                },
              },
            ],
            rehypeTargetBlank,
            [rehypeStringify as () => void, { allowDangerousHtml: true }],
          ],
        });
        const compiledModule = await run(code, { ...(runtime as any) });
        setMdxModule(compiledModule);
      } catch (error) {
        console.error('Error compiling MDX:', error);
        throw error;
      }
    })();
  }, [source, highlighter, theme]);

  return (
    <div
      className={cn(
        'prose max-w-none text-xs',
        theme === 'dark' ? 'prose-invert' : 'prose-light',
      )}
    >
      <Content />
    </div>
  );
}
