/**
 * Loads the packaged playground HTML, rewrites Vite asset URLs to VS Code
 * webview URIs, and injects the websocket URL for the Rust playground server.
 */

import { promises as fs } from 'fs';
import { Uri, type Webview } from 'vscode';

export async function getPlaygroundHtml(webview: Webview, extensionUri: Uri, port: number): Promise<string> {
  const playgroundRoot = Uri.joinPath(extensionUri, 'dist', 'playground');
  const indexPath = Uri.joinPath(playgroundRoot, 'index.html').fsPath;
  let html = await fs.readFile(indexPath, 'utf8');

  // Strip any existing CSP meta tag from the fetched HTML.
  html = html.replace(/<meta[^>]*Content-Security-Policy[^>]*>/gi, '');

  html = html.replace(
    /\b(src|href)=["']\/assets\/([^"']+)["']/g,
    (_match, attr: string, assetName: string) => {
      const assetUri = webview.asWebviewUri(Uri.joinPath(playgroundRoot, 'assets', assetName));
      return `${attr}="${assetUri.toString()}"`;
    },
  );

  // Inject CSP and a global WS URL right after <head>.
  const inject = [
    `<meta http-equiv="Content-Security-Policy"`,
    `      content="default-src 'none';`,
    `               script-src ${webview.cspSource} 'unsafe-inline';`,
    `               style-src ${webview.cspSource} 'unsafe-inline';`,
    `               connect-src ws://localhost:* http://localhost:*;`,
    `               img-src data: ${webview.cspSource} http://localhost:*;`,
    `               font-src ${webview.cspSource};" />`,
    `<script>window.__PLAYGROUND_WS_URL = "ws://localhost:${port}/api/ws";</script>`,
  ].join('\n    ');

  html = html.replace(/<head[^>]*>/i, (m) => `${m}\n    ${inject}`);
  return html;
}
