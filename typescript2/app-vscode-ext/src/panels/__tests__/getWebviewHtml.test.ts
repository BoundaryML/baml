import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('vscode', () => {
  const makeUri = (fsPath: string) => ({
    fsPath,
    path: fsPath,
    toString: () => fsPath,
  });

  return {
    Uri: {
      file: makeUri,
      parse: makeUri,
      joinPath: (base: { fsPath: string }, ...paths: string[]) => makeUri(
        [base.fsPath.replace(/\/$/, ''), ...paths].join('/'),
      ),
    },
  };
});

import { getPlaygroundHtml } from '../getWebviewHtml';
import { promises as fs } from 'fs';
import { Uri, type Webview } from 'vscode';

vi.mock('fs', () => ({
  promises: {
    readFile: vi.fn(),
  },
}));

const FAKE_HTML = `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'self'" />
    <title>Playground</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/assets/index.js"></script>
    <link rel="stylesheet" href="/assets/index.css">
  </body>
</html>`;

const webview = {
  cspSource: 'vscode-webview://test',
  asWebviewUri: (uri: Uri) => Uri.parse(`vscode-webview://test/${uri.path.split('/dist/playground/')[1]}`),
} as Webview;

const extensionUri = Uri.file('/extension');

beforeEach(() => {
  vi.mocked(fs.readFile).mockResolvedValue(FAKE_HTML);
});

describe('getPlaygroundHtml', () => {
  it('reads the packaged playground HTML', async () => {
    await getPlaygroundHtml(webview, extensionUri, 4265);

    expect(fs.readFile).toHaveBeenCalledWith('/extension/dist/playground/index.html', 'utf8');
  });

  it('injects the WS URL global', async () => {
    const html = await getPlaygroundHtml(webview, extensionUri, 4265);

    expect(html).toContain('__PLAYGROUND_WS_URL');
    expect(html).toContain('ws://localhost:4265/api/ws');
  });

  it('injects a CSP allowing packaged scripts and localhost websocket traffic', async () => {
    const html = await getPlaygroundHtml(webview, extensionUri, 4265);

    expect(html).toContain('script-src vscode-webview://test');
    expect(html).toContain('connect-src ws://localhost:*');
    expect(html).not.toContain(`default-src 'self'`);
  });

  it('does not contain an iframe', async () => {
    const html = await getPlaygroundHtml(webview, extensionUri, 4265);

    expect(html).not.toContain('<iframe');
  });

  it('rewrites Vite asset URLs to webview URIs', async () => {
    const html = await getPlaygroundHtml(webview, extensionUri, 4265);

    expect(html).toContain('<div id="root"></div>');
    expect(html).toContain('src="vscode-webview://test/assets/index.js"');
    expect(html).toContain('href="vscode-webview://test/assets/index.css"');
  });
});
