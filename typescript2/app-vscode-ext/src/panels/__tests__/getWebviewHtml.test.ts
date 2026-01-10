import { describe, it, expect } from 'vitest';
import { getWebviewHtml } from '../getWebviewHtml';

const baseOptions = {
  devServerUrl: 'localhost:4000',
  scriptUri: 'vscode-resource://extension/dist/assets/index.js',
  stylesUri: 'vscode-resource://extension/dist/assets/index.css',
  cspSource: 'vscode-webview:',
  nonce: 'test-nonce-12345',
};

describe('getWebviewHtml', () => {
  it('generates correct HTML in development mode', () => {
    const devHtml = getWebviewHtml({
      ...baseOptions,
      isDevelopment: true,
      scriptUri: 'http://localhost:4000/src/main.tsx',
      stylesUri: 'http://localhost:4000/src/index.css',
    });

    expect(devHtml).toMatchInlineSnapshot(`
      "<!DOCTYPE html>
          <html lang="en">
            <head>
              <script type="module" nonce="test-nonce-12345">
                import { injectIntoGlobalHook } from "http://localhost:4000/@react-refresh";
                injectIntoGlobalHook(window);
                window.$RefreshReg$ = () => {};
                window.$RefreshSig$ = () => (type) => type;
              </script>
              <script type="module" nonce="test-nonce-12345" src="http://localhost:4000/@vite/client"></script>
              <meta charset="UTF-8" />
              <meta name="viewport" content="width=device-width, initial-scale=1.0" />
              <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-eval' http://localhost:4000 'nonce-test-nonce-12345'; style-src vscode-webview: 'self' 'unsafe-inline' http://localhost:4000; font-src vscode-webview:; connect-src ws://localhost:4000 http://localhost:4000; img-src vscode-webview: https: data:">
              
              <title>BAML Playground</title>
            </head>
            <body>
              <div id="root"></div>
              <script type="module"  src="http://localhost:4000/src/main.tsx"></script>
            </body>
          </html>"
    `);
  });

  it('generates correct HTML in production mode', () => {
    const prodHtml = getWebviewHtml({
      ...baseOptions,
      isDevelopment: false,
    });

    expect(prodHtml).toMatchInlineSnapshot(`
      "<!DOCTYPE html>
          <html lang="en">
            <head>
              
              <meta charset="UTF-8" />
              <meta name="viewport" content="width=device-width, initial-scale=1.0" />
              <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-eval' 'nonce-test-nonce-12345'; style-src vscode-webview: 'self' 'unsafe-inline' ; font-src vscode-webview:; connect-src ; img-src vscode-webview: https: data:">
              <link rel="stylesheet" type="text/css" href="vscode-resource://extension/dist/assets/index.css">
              <title>BAML Playground</title>
            </head>
            <body>
              <div id="root"></div>
              <script type="module" nonce="test-nonce-12345" src="vscode-resource://extension/dist/assets/index.js"></script>
            </body>
          </html>"
    `);
  });
});
