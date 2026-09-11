import { Children, isValidElement, type ReactNode } from 'react';
import { AnnotatedSnippet } from '@/components/annotated-snippet';
import { CodeBlock } from '@/components/code-block';
import { CodeHighlightKey, CodeTokens } from '@/components/code-tokens';
import type { CodeAnnotationId } from '@/lib/snippets/annotation-specs';
import {
  type CodeHighlight,
  resolveCodeHighlights,
} from '@/lib/snippets/code-highlights';
import { highlightCode } from '@/lib/snippets/highlighter';

function codeText(node: ReactNode): string {
  return Children.toArray(node)
    .map((child) => {
      // oxlint-disable-next-line anti-slop/no-runtime-typeof -- ReactNode is a text/element union; extract text leaves from the MDX code fence.
      if (typeof child === 'string' || typeof child === 'number')
        return String(child);
      if (isValidElement<{ children?: ReactNode }>(child))
        return codeText(child.props.children);
      return '';
    })
    .join('');
}

export async function CodeExample({
  children,
  language,
  highlights = [],
  annotation,
}: {
  children: ReactNode;
  language: 'typescript' | 'rust';
  highlights?: CodeHighlight[];
  annotation?: CodeAnnotationId;
}) {
  const code = codeText(children).trim();
  if (annotation)
    return (
      <AnnotatedSnippet
        annotation={annotation}
        code={code}
        language={language}
      />
    );
  const tokens = await highlightCode(code, language);
  const ranges = resolveCodeHighlights(code, highlights);
  return (
    <div className="code-example">
      <CodeBlock source={code}>
        <CodeTokens
          className="baml-code-light"
          lines={tokens.light}
          ranges={ranges}
        />
        <CodeTokens
          className="baml-code-dark"
          lines={tokens.dark}
          ranges={ranges}
        />
      </CodeBlock>
      <CodeHighlightKey highlights={highlights} />
    </div>
  );
}
