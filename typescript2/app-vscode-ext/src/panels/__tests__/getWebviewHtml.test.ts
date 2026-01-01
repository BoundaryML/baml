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
  describe('development mode', () => {
    const devHtml = getWebviewHtml({
      ...baseOptions,
      isDevelopment: true,
      scriptUri: 'http://localhost:4000/src/main.tsx',
      stylesUri: 'http://localhost:4000/src/index.css',
    });

    it('includes Vite client script', () => {
      expect(devHtml).toContain('http://localhost:4000/@vite/client');
    });

    it('includes React Refresh script', () => {
      expect(devHtml).toContain('http://localhost:4000/@react-refresh');
      expect(devHtml).toContain('injectIntoGlobalHook');
    });

    it('loads script from dev server', () => {
      expect(devHtml).toContain('src="http://localhost:4000/src/main.tsx"');
    });

    it('does not include CSS link tag (Vite injects CSS)', () => {
      expect(devHtml).not.toContain('<link rel="stylesheet"');
    });

    it('CSP allows localhost connections', () => {
      expect(devHtml).toContain('connect-src ws://localhost:4000 http://localhost:4000');
      expect(devHtml).toContain('script-src \'unsafe-eval\' http://localhost:4000');
    });

    it('script tag has no nonce in dev mode', () => {
      expect(devHtml).toContain('<script type="module"  src="http://localhost:4000/src/main.tsx">');
    });
  });

  describe('production mode', () => {
    const prodHtml = getWebviewHtml({
      ...baseOptions,
      isDevelopment: false,
    });

    it('does not include Vite client script', () => {
      expect(prodHtml).not.toContain('@vite/client');
    });

    it('does not include React Refresh script', () => {
      expect(prodHtml).not.toContain('@react-refresh');
      expect(prodHtml).not.toContain('injectIntoGlobalHook');
    });

    it('loads script from bundled assets', () => {
      expect(prodHtml).toContain('src="vscode-resource://extension/dist/assets/index.js"');
    });

    it('includes CSS link tag', () => {
      expect(prodHtml).toContain('<link rel="stylesheet" type="text/css" href="vscode-resource://extension/dist/assets/index.css">');
    });

    it('CSP does not allow localhost connections', () => {
      expect(prodHtml).not.toContain('ws://localhost');
      expect(prodHtml).not.toContain('http://localhost');
    });

    it('script tag has nonce in prod mode', () => {
      expect(prodHtml).toContain(`nonce="${baseOptions.nonce}"`);
    });
  });

  describe('common elements', () => {
    const html = getWebviewHtml({ ...baseOptions, isDevelopment: false });

    it('includes root div', () => {
      expect(html).toContain('<div id="root"></div>');
    });

    it('includes viewport meta tag', () => {
      expect(html).toContain('<meta name="viewport"');
    });

    it('includes Content-Security-Policy', () => {
      expect(html).toContain('Content-Security-Policy');
    });

    it('includes title', () => {
      expect(html).toContain('<title>BAML Playground</title>');
    });
  });
});
