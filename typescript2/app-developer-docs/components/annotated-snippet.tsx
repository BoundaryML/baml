import { AnnotatedCode } from '@/components/annotated-code';
import { CodeTokens } from '@/components/code-tokens';
import { getAnnotationMetadata } from '@/lib/snippets/annotation-metadata';
import type { CodeAnnotationId } from '@/lib/snippets/annotation-specs';
import { highlightCode } from '@/lib/snippets/highlighter';

export async function AnnotatedSnippet({
  code,
  annotation,
  filename,
  language,
}: {
  code: string;
  annotation: CodeAnnotationId;
  filename?: string;
  language: Parameters<typeof highlightCode>[1];
}) {
  const metadata = getAnnotationMetadata(annotation, code);
  const tokens = await highlightCode(code, language);
  return (
    <AnnotatedCode
      {...metadata}
      code={code}
      filename={filename}
      id={annotation}
    >
      {(['light', 'dark'] as const).map((theme) => (
        <pre className={`annotated-code-plain baml-code-${theme}`} key={theme}>
          <CodeTokens lines={tokens[theme]} ranges={[]} />
        </pre>
      ))}
    </AnnotatedCode>
  );
}
