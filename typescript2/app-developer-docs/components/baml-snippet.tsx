import {
  loadProjectSnippet,
  loadStandaloneSnippet,
} from '@/lib/snippets/discovery';
import { highlightCode } from '@/lib/snippets/highlighter';

function TokenizedCode({
  className,
  lines,
}: {
  className: string;
  lines: Awaited<ReturnType<typeof highlightCode>>['light'];
}) {
  let nextLineOffset = 0;
  const keyedLines = lines.map((line) => {
    const offset = nextLineOffset;
    nextLineOffset +=
      line.reduce((length, token) => length + token.content.length, 0) + 1;
    return { line, offset };
  });
  return (
    <pre className={className}>
      <code>
        {keyedLines.map(({ line, offset }, lineIndex) => (
          <span key={offset}>
            {line.map((token) => (
              <span key={token.offset} style={{ color: token.color }}>
                {token.content}
              </span>
            ))}
            {lineIndex === keyedLines.length - 1 ? null : '\n'}
          </span>
        ))}
      </code>
    </pre>
  );
}

async function HighlightedCode({
  code,
  filename,
  language,
}: {
  code: string;
  filename: string;
  language: 'baml' | 'toml';
}) {
  const tokens = await highlightCode(code, language);
  return (
    <figure className="baml-code" data-language={language}>
      <figcaption>{filename}</figcaption>
      <TokenizedCode className="baml-code-light" lines={tokens.light} />
      <TokenizedCode className="baml-code-dark" lines={tokens.dark} />
    </figure>
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
      <HighlightedCode code={code} filename={`${id}.baml`} language="baml" />
    </div>
  );
}

export async function BamlProject({ id }: { id: string }) {
  const project = await loadProjectSnippet(id);
  return (
    <div className="baml-project" data-project-id={id}>
      {project.files.map((file) => (
        <HighlightedCode
          code={file.displaySource}
          filename={file.projectPath}
          key={file.projectPath}
          language={file.projectPath.endsWith('.toml') ? 'toml' : 'baml'}
        />
      ))}
    </div>
  );
}
