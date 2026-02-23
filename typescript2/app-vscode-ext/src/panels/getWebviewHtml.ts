/**
 * Fetches the playground HTML from the Rust server, then injects a <base> tag
 * and CSP so it can run directly inside a VS Code webview (no iframe).
 *
 * The <base> tag makes all relative asset URLs (scripts, styles) resolve
 * against the playground server. This avoids the cross-origin clipboard and
 * keyboard issues that plague the iframe approach.
 */

import * as http from 'http';

export async function getPlaygroundHtml(port: number): Promise<string> {
  let html = await fetchHtml(port);

  // Strip any existing CSP meta tag from the fetched HTML.
  html = html.replace(/<meta[^>]*Content-Security-Policy[^>]*>/gi, '');

  // Inject <base>, CSP, and a global WS URL right after <head>.
  const inject = [
    `<base href="http://localhost:${port}/" />`,
    `<meta http-equiv="Content-Security-Policy"`,
    `      content="default-src 'none';`,
    `               script-src http://localhost:* 'unsafe-inline' 'unsafe-eval';`,
    `               style-src  http://localhost:* 'unsafe-inline';`,
    `               connect-src ws://localhost:* http://localhost:*;`,
    `               img-src data: http://localhost:*;`,
    `               font-src http://localhost:*;" />`,
    `<script>window.__PLAYGROUND_WS_URL = "ws://localhost:${port}/api/ws";</script>`,
  ].join('\n    ');

  html = html.replace(/<head[^>]*>/i, (m) => `${m}\n    ${inject}`);
  return html;
}

// ── helpers ────────────────────────────────────────────────────────────────

function fetchHtml(port: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const req = http.get(`http://localhost:${port}/`, (res) => {
      let data = '';
      res.on('data', (chunk: string) => (data += chunk));
      res.on('end', () => resolve(data));
    });
    req.on('error', reject);
    req.setTimeout(5000, () => {
      req.destroy();
      reject(new Error('Timeout fetching playground HTML'));
    });
  });
}
