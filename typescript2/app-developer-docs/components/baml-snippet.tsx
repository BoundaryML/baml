import Image from 'next/image';
import { AnnotatedSnippet } from '@/components/annotated-snippet';
import { CodeHighlightKey, CodeTokens } from '@/components/code-tokens';
import type { CodeAnnotationId } from '@/lib/snippets/annotation-specs';
import {
  type CodeHighlight,
  resolveCodeHighlights,
} from '@/lib/snippets/code-highlights';

import {
  loadProjectSnippet,
  loadStandaloneSnippet,
} from '@/lib/snippets/discovery';
import { highlightCode } from '@/lib/snippets/highlighter';
import { selectProjectFiles } from '@/lib/snippets/selection';

async function HighlightedCode({
  code,
  filename,
  language,
  excerpt = false,
  highlights = [],
  annotation,
}: {
  code: string;
  filename: string;
  language: 'baml' | 'toml';
  excerpt?: boolean;
  highlights?: CodeHighlight[];
  annotation?: CodeAnnotationId;
}) {
  if (annotation)
    return (
      <AnnotatedSnippet
        annotation={annotation}
        code={code}
        filename={filename}
        language={language}
      />
    );
  const tokens = await highlightCode(code, language);
  const ranges = resolveCodeHighlights(code, highlights);
  return (
    <>
      <figure className="baml-code not-typeset" data-language={language}>
        <figcaption>
          {language === 'baml' ? (
            <span className="baml-code-tab">
              <Image
                alt="BAML"
                className="baml-code-mark"
                height={14}
                src="/baml-logo.png"
                width={14}
              />
              <span className="baml-code-filename" title={filename}>
                {excerpt ? filename.split('/').at(-1) : filename}
              </span>
            </span>
          ) : (
            <span className="baml-code-filename" title={filename}>
              {filename}
            </span>
          )}
        </figcaption>
        <pre className="baml-code-light">
          <CodeTokens lines={tokens.light} ranges={ranges} />
        </pre>
        <pre className="baml-code-dark">
          <CodeTokens lines={tokens.dark} ranges={ranges} />
        </pre>
      </figure>
      {highlights.length > 0 ? (
        <CodeHighlightKey highlights={highlights} />
      ) : null}
    </>
  );
}

export async function BamlSnippet({
  id,
  region = 'example',
}: {
  id: string;
  region?: string;
}) {
  const snippet = await loadStandaloneSnippet(id);
  const code = snippet.parsed.regions.get(region);
  if (code === undefined) {
    const available = [...snippet.parsed.regions.keys()].join(', ');
    throw new Error(
      `BAML snippet ${id} has no region ${region}. Available regions: ${available}`,
    );
  }

  return (
    <div className="baml-snippet" data-snippet-id={id}>
      <HighlightedCode
        code={code}
        excerpt
        filename={`${id}.baml`}
        language="baml"
      />
    </div>
  );
}

export async function BamlProject({
  id,
  file,
  regions,
  highlights,
  annotation,
}: {
  id: string;
  file?: string;
  regions?: string[];
  highlights?: CodeHighlight[];
  annotation?: CodeAnnotationId;
}) {
  const project = await loadProjectSnippet(id);
  const files = selectProjectFiles(project, file, regions);
  return (
    <div className="baml-project" data-project-id={id}>
      {files.map((file) => (
        <HighlightedCode
          annotation={annotation}
          code={file.displaySource}
          excerpt={regions !== undefined}
          filename={file.projectPath}
          highlights={highlights}
          key={file.projectPath}
          language={file.projectPath.endsWith('.toml') ? 'toml' : 'baml'}
        />
      ))}
    </div>
  );
}
