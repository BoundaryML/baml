/**
 * Renders a baml.http.Request as curl, JS fetch, Python requests, Go, or Rust.
 */

import type { FC } from 'react';
import { useCallback, useMemo, useState } from 'react';
import { Highlight, themes } from 'prism-react-renderer';
import type { ResultRendererProps } from '../result-renderers';
import { ToggleGroup } from '../components/ui/toggle-group';
import { Button } from '../components/ui/button';

interface HttpRequestShape {
  method?: string;
  url?: string;
  headers?: Record<string, string>;
  body?: string;
}

/**
 * Whether the body must not be reformatted (e.g. SigV4-signed requests where
 * the body hash is part of the signature).
 *
 * TODO: maybe pass a flag through BAML that tells us if we need to preserve
 * the exact request body?
 */
function preserveExactBody(req: HttpRequestShape): boolean {
  return (req.url ?? '').includes('bedrock-runtime');
}

export function isHttpRequest(value: unknown): value is HttpRequestShape {
  if (value == null || typeof value !== 'object') return false;
  const o = value as Record<string, unknown>;
  return typeof o.url === 'string' && typeof o.method === 'string';
}

function safeHeredoc(body: string): string {
  let tag = 'EOF';
  while (body.includes(`\n${tag}\n`) || body.startsWith(`${tag}\n`) || body.endsWith(`\n${tag}`)) {
    tag += '_';
  }
  return tag;
}

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
      const escapedKey = String(k).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
      const escapedVal = String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
      parts.push(`-H "${escapedKey}: ${escapedVal}"`);
    }
  }
  if (body != null && body !== '') {
    const preserve = preserveExactBody(req);
    const formatted = preserve ? body : prettyBody(body);
    if (preserve) {
      const escaped = formatted.replace(/'/g, "'\\''");
      parts.push(`-d '${escaped}'`);
    } else {
      const heredoc = safeHeredoc(formatted);
      parts.push('-d @-');
      parts.push(`'${url.replace(/'/g, "'\\''")}'`);
      return parts.join(' \\\n  ') + ` <<'${heredoc}'\n${formatted}\n${heredoc}`;
    }
  }
  parts.push(`'${url.replace(/'/g, "'\\''")}'`);
  return parts.join(' \\\n  ');
}

function escapeJsSingle(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/'/g, "\\'");
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
      .map(([k, v]) => `    '${escapeJsSingle(k)}': '${escapeJsSingle(String(v))}'`)
      .join(',\n');
    opts.push(`  headers: {\n${headersStr}\n  }`);
  }
  if (body != null && body !== '') {
    const preserve = preserveExactBody(req);
    const formatted = preserve ? body : prettyBody(body);
    const escaped = formatted.replace(/\\/g, '\\\\').replace(/`/g, '\\`').replace(/\$/g, '\\$');
    opts.push(`  body: \`${escaped}\``);
  }
  return `fetch('${escapeJsSingle(url)}', {\n${opts.join(',\n')}\n});`;
}

function escapePySingle(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/'/g, "\\'");
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
      .map(([k, v]) => `        '${escapePySingle(k)}': '${escapePySingle(String(v))}'`)
      .join(',\n');
    kwargs.push(`    headers={\n${headersStr}\n    }`);
  }
  if (body != null && body !== '') {
    const preserve = preserveExactBody(req);
    const formatted = preserve ? body : prettyBody(body);
    if (preserve) {
      const escaped = formatted.replace(/'/g, "\\'");
      kwargs.push(`    data='${escaped}'`);
    } else {
      const safeBody = formatted.replace(/'''/g, "\\'\\'\\'");
      kwargs.push(`    data='''\n${safeBody}\n    '''`);
    }
  }
  const kwargsBlock = kwargs.length ? ',\n' + kwargs.join(',\n') : '';
  return `import requests\n\nresponse = requests.${fn}(\n    ${args}'${escapePySingle(url)}'${kwargsBlock}\n)`;
}

function escapeGoString(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r');
}

function toGo(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toUpperCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body ?? '';
  const bodyFormatted = body ? (preserveExactBody(req) ? body : prettyBody(body)) : '';
  const bodyArg = body ? `strings.NewReader("${escapeGoString(bodyFormatted)}")` : 'nil';
  const lines: string[] = [
    `req, err := http.NewRequest("${method}", "${escapeGoString(url)}", ${bodyArg})`,
    'if err != nil { log.Fatal(err) }',
  ];
  if (body) {
    lines.push('defer req.Body.Close()');
  }
  for (const [k, v] of Object.entries(headers)) {
    if (v != null) lines.push(`req.Header.Set("${escapeGoString(k)}", "${escapeGoString(String(v))}")`);
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

function escapeRustString(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r');
}

const REQWEST_SHORTCUT_METHODS = new Set(['get', 'post', 'put', 'patch', 'delete', 'head']);

function toRust(req: HttpRequestShape): string {
  const method = (req.method ?? 'GET').toLowerCase();
  const url = req.url ?? '';
  const headers = req.headers ?? {};
  const body = req.body ?? '';
  const bodyFormatted = body ? (preserveExactBody(req) ? body : prettyBody(body)) : '';
  const clientCall = REQWEST_SHORTCUT_METHODS.has(method)
    ? `client.${method}("${escapeRustString(url)}")`
    : `client.request("${method.toUpperCase()}".parse().unwrap(), "${escapeRustString(url)}")`;
  const lines: string[] = [
    'let client = reqwest::blocking::Client::new();',
    `let response = ${clientCall}`,
  ];
  for (const [k, v] of Object.entries(headers)) {
    if (v != null) lines.push(`    .header("${escapeRustString(k)}", "${escapeRustString(String(v))}")`);
  }
  if (body) {
    const preserve = preserveExactBody(req);
    if (preserve) {
      const escaped = bodyFormatted.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
      lines.push(`    .body("${escaped}")`);
    } else {
      let maxHashes = 0;
      const re = /"(#+)/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(bodyFormatted)) !== null) {
        maxHashes = Math.max(maxHashes, m[1].length);
      }
      const hashCount = maxHashes + 1;
      const hash = '#'.repeat(hashCount);
      lines.push(`    .body(r${hash}"\n${bodyFormatted}\n"${hash})`);
    }
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

export const HttpRequestCurlRenderer: FC<ResultRendererProps> = ({ value, displayMode }) => {
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

  if (displayMode === 'inline') {
    return <span className="font-vsc-mono text-xs text-vsc-text">{httpReq.method} {httpReq.url}</span>;
  }

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-1">
        <ToggleGroup
          value={format}
          onValueChange={setFormat}
          options={FORMATS.map(({ id, label }) => ({ value: id, label }))}
          size="sm"
        />
        <Button
          variant="outline"
          size="sm"
          className="ml-auto text-[10px]"
          onClick={onCopy}
        >
          {copied ? 'Copied' : 'Copy'}
        </Button>
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
