/**
 * Renders a baml.http.Request as curl, JS fetch, Python requests, Go, or Rust.
 */

import type { FC } from 'react';
import { useCallback, useMemo, useState } from 'react';
import { Highlight, themes } from 'prism-react-renderer';
import type { ResultRendererProps } from '../result-renderers';

interface HttpRequestShape {
  method?: string;
  url?: string;
  headers?: Record<string, string>;
  body?: string;
}

function isHttpRequest(value: unknown): value is HttpRequestShape {
  if (value == null || typeof value !== 'object') return false;
  const o = value as Record<string, unknown>;
  return typeof o.url === 'string' && typeof o.method === 'string';
}

const HEREDOC = 'EOF';

const PYTHON_METHODS = new Set(['get', 'post', 'put', 'patch', 'delete']);

/** Pretty-print JSON when possible so snippets are readable. */
function prettyBody(body: string): string {
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
}

function toCurl(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toUpperCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body;
  const parts: string[] = ['curl'];
  if (method !== 'GET') parts.push(`-X ${method}`);
  for (const [k, v] of Object.entries(headers)) {
    if (v !== undefined && v !== null) {
      const escaped = String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
      parts.push(`-H "${String(k)}: ${escaped}"`);
    }
  }
  if (body != null && body !== '') {
    parts.push('-d @-');
    parts.push(`'${url.replace(/'/g, "'\\''")}'`);
    return parts.join(' \\\n  ') + ` <<'${HEREDOC}'\n${prettyBody(body)}\n${HEREDOC}`;
  }
  parts.push(`'${url.replace(/'/g, "'\\''")}'`);
  return parts.join(' \\\n  ');
}

function toJsFetch(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toUpperCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body;
  const opts: string[] = [`  method: '${method}'`];
  if (Object.keys(headers).length > 0) {
    const headersStr = Object.entries(headers)
      .filter(([, v]) => v != null)
      .map(([k, v]) => `    '${k.replace(/'/g, "\\'")}': '${String(v).replace(/'/g, "\\'")}'`)
      .join(',\n');
    opts.push(`  headers: {\n${headersStr}\n  }`);
  }
  if (body != null && body !== '') {
    const formatted = prettyBody(body);
    const escaped = formatted.replace(/\\/g, '\\\\').replace(/`/g, '\\`').replace(/\$/g, '\\$');
    opts.push(`  body: \`${escaped}\``);
  }
  return `fetch('${url.replace(/'/g, "\\'")}', {\n${opts.join(',\n')}\n});`;
}

function toPythonRequests(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toLowerCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body;
  const fn = PYTHON_METHODS.has(method) ? method : 'request';
  const args = fn === 'request' ? `'${method}', ` : '';
  const kwargs: string[] = [];
  if (Object.keys(headers).length > 0) {
    const headersStr = Object.entries(headers)
      .filter(([, v]) => v != null)
      .map(([k, v]) => `        '${k.replace(/'/g, "\\'")}': '${String(v).replace(/'/g, "\\'")}'`)
      .join(',\n');
    kwargs.push(`    headers={\n${headersStr}\n    }`);
  }
  if (body != null && body !== '') {
    kwargs.push(`    data='''\n${prettyBody(body)}\n    '''`);
  }
  const kwargsBlock = kwargs.length ? ',\n' + kwargs.join(',\n') : '';
  return `import requests\n\nresponse = requests.${fn}(\n    ${args}'${url.replace(/'/g, "\\'")}'${kwargsBlock}\n)`;
}

function escapeGoString(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r');
}

function toGo(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toUpperCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body ?? '';
  const bodyArg = body ? `strings.NewReader("${escapeGoString(prettyBody(body))}")` : 'nil';
  const lines: string[] = [
    `req, err := http.NewRequest("${method}", "${url.replace(/"/g, '\\"')}", ${bodyArg})`,
    'if err != nil { log.Fatal(err) }',
  ];
  if (body) {
    lines.push('defer req.Body.Close()');
  }
  for (const [k, v] of Object.entries(headers)) {
    if (v != null) lines.push(`req.Header.Set("${k.replace(/"/g, '\\"')}", "${String(v).replace(/"/g, '\\"')}")`);
  }
  lines.push('resp, err := http.DefaultClient.Do(req)');
  lines.push('if err != nil { log.Fatal(err) }');
  lines.push('defer resp.Body.Close()');
  return (
    'package main\n\nimport (\n\t"net/http"\n\t"log"\n' +
    (body ? '\t"strings"\n' : '') +
    ')\n\nfunc main() {\n\t' +
    lines.join('\n\t') +
    '\n}'
  );
}

function toRust(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toLowerCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body ?? '';
  const bodyFormatted = body ? prettyBody(body) : '';
  const lines: string[] = [
    'let client = reqwest::blocking::Client::new();',
    `let response = client.${method}("${url.replace(/"/g, '\\"')}")`,
  ];
  for (const [k, v] of Object.entries(headers)) {
    if (v != null) lines.push(`    .header("${k.replace(/"/g, '\\"')}", "${String(v).replace(/"/g, '\\"')}")`);
  }
  if (body) {
    const hashCount = bodyFormatted.includes('"#') ? 2 : 1;
    const hash = '#'.repeat(hashCount);
    const safeForTemplate = bodyFormatted.replace(/`/g, '\\`').replace(/\$/g, '\\$');
    lines.push(`    .body(r${hash}"\n${safeForTemplate}\n"${hash})`);
  }
  lines.push('    .send()?;');
  return '// reqwest = { version = "0.11", features = ["blocking"] }\n\n' + lines.join('\n');
}

export type HttpRequestSnippetFormat = 'curl' | 'fetch' | 'python' | 'go' | 'rust';

const FORMATS: { id: HttpRequestSnippetFormat; label: string }[] = [
  { id: 'curl', label: 'curl' },
  { id: 'fetch', label: 'JS fetch' },
  { id: 'python', label: 'Python' },
  { id: 'go', label: 'Go' },
  { id: 'rust', label: 'Rust' },
];

const FORMAT_TO_LANGUAGE: Record<HttpRequestSnippetFormat, string> = {
  curl: 'bash',
  fetch: 'javascript',
  python: 'python',
  go: 'go',
  rust: 'rust',
};

function formatSnippet(req: HttpRequestShape, format: HttpRequestSnippetFormat): string {
  switch (format) {
    case 'curl':
      return toCurl(req);
    case 'fetch':
      return toJsFetch(req);
    case 'python':
      return toPythonRequests(req);
    case 'go':
      return toGo(req);
    case 'rust':
      return toRust(req);
    default:
      return toCurl(req);
  }
}

/** @deprecated Use formatSnippet(req, 'curl') or the format switcher in the UI. */
export function httpRequestToCurl(req: HttpRequestShape): string {
  return toCurl(req);
}

const preCls =
  'whitespace-pre font-vsc-mono text-xs leading-relaxed p-3 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[400px] m-0';

const tabCls = (active: boolean) =>
  active
    ? 'px-1.5 py-0.5 rounded font-vsc-mono text-[10px] cursor-pointer border bg-vsc-accent text-vsc-accent-fg border-vsc-accent'
    : 'px-1.5 py-0.5 rounded font-vsc-mono text-[10px] cursor-pointer border border-vsc-border bg-vsc-surface text-vsc-text-muted hover:bg-vsc-list-hoverBg';

export const HttpRequestCurlRenderer: FC<ResultRendererProps> = ({ value }) => {
  const [copied, setCopied] = useState(false);
  const [format, setFormat] = useState<HttpRequestSnippetFormat>('curl');
  const httpReq = isHttpRequest(value) ? value : null;

  const snippet = useMemo(
    () => (httpReq ? formatSnippet(httpReq, format) : ''),
    [httpReq, format],
  );

  const onCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(snippet);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // ignore
    }
  }, [snippet]);

  if (!httpReq) {
    return <pre className={preCls}>{JSON.stringify(value, null, 2)}</pre>;
  }

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-1">
        {FORMATS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => setFormat(id)}
            className={tabCls(format === id)}
          >
            {label}
          </button>
        ))}
        <button
          type="button"
          onClick={onCopy}
          className="ml-auto px-1.5 py-0.5 rounded border border-vsc-border bg-vsc-surface text-vsc-text-muted text-[10px] cursor-pointer hover:bg-vsc-accent hover:text-vsc-accent-fg"
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <Highlight
        theme={themes.vsDark}
        code={snippet}
        language={FORMAT_TO_LANGUAGE[format]}
      >
        {({ className, style, tokens, getLineProps, getTokenProps }) => (
          <pre className={`${preCls} ${className}`} style={style}>
            {tokens.map((line, i) => (
              <div key={i} {...getLineProps({ line })}>
                {line.map((token, key) => (
                  <span key={key} {...getTokenProps({ token })} />
                ))}
              </div>
            ))}
          </pre>
        )}
      </Highlight>
    </div>
  );
};
