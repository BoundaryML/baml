import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getPlaygroundHtml } from '../getWebviewHtml';
import * as http from 'http';

// Stub http.get to return fake HTML without hitting a real server.
vi.mock('http', () => ({
  get: vi.fn(),
}));

const FAKE_HTML = `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>Playground</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/assets/index.js"></script>
  </body>
</html>`;

beforeEach(() => {
  // Simulate a successful HTTP response.
  (http.get as ReturnType<typeof vi.fn>).mockImplementation((_url: string, cb: (res: any) => void) => {
    const res = {
      on(event: string, handler: (data?: string) => void) {
        if (event === 'data') handler(FAKE_HTML);
        if (event === 'end') handler();
        return res;
      },
    };
    cb(res);
    return { on: () => ({}), setTimeout: () => ({}) };
  });
});

describe('getPlaygroundHtml', () => {
  it('injects a base tag pointing at the server', async () => {
    const html = await getPlaygroundHtml(3700);

    expect(html).toContain('<base href="http://localhost:3700/"');
  });

  it('injects the WS URL global', async () => {
    const html = await getPlaygroundHtml(3700);

    expect(html).toContain('__PLAYGROUND_WS_URL');
    expect(html).toContain('ws://localhost:3700/api/ws');
  });

  it('injects a CSP allowing localhost scripts', async () => {
    const html = await getPlaygroundHtml(3700);

    expect(html).toContain('script-src http://localhost:*');
    expect(html).toContain('connect-src ws://localhost:*');
  });

  it('does not contain an iframe', async () => {
    const html = await getPlaygroundHtml(3700);

    expect(html).not.toContain('<iframe');
  });

  it('preserves the original body content', async () => {
    const html = await getPlaygroundHtml(3700);

    expect(html).toContain('<div id="root"></div>');
    expect(html).toContain('/assets/index.js');
  });
});
