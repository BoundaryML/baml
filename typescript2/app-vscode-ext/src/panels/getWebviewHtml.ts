import { getNonce } from '../utils/getNonce';

export interface WebviewHtmlOptions {
  isDevelopment: boolean;
  devServerUrl: string;
  scriptUri: string;
  stylesUri: string;
  cspSource: string;
  nonce?: string;
}

export function getWebviewHtml(options: WebviewHtmlOptions): string {
  const {
    isDevelopment,
    devServerUrl,
    scriptUri,
    stylesUri,
    cspSource,
    nonce = getNonce(),
  } = options;

  const viteScripts = isDevelopment
    ? `<script type="module" nonce="${nonce}">
          import { injectIntoGlobalHook } from "http://${devServerUrl}/@react-refresh";
          injectIntoGlobalHook(window);
          window.$RefreshReg$ = () => {};
          window.$RefreshSig$ = () => (type) => type;
        </script>
        <script type="module" nonce="${nonce}" src="http://${devServerUrl}/@vite/client"></script>`
    : '';

  const csp = [
    `default-src 'none'`,
    `script-src 'unsafe-eval' ${
      isDevelopment
        ? `http://${devServerUrl} 'nonce-${nonce}'`
        : `'nonce-${nonce}'`
    }`,
    `style-src ${cspSource} 'self' 'unsafe-inline' ${
      isDevelopment ? `http://${devServerUrl}` : ''
    }`,
    `font-src ${cspSource}`,
    `connect-src ${
      isDevelopment ? `ws://${devServerUrl} http://${devServerUrl}` : ''
    }`,
    `img-src ${cspSource} https: data:`,
  ];

  return `<!DOCTYPE html>
    <html lang="en">
      <head>
        ${viteScripts}
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <meta http-equiv="Content-Security-Policy" content="${csp.join('; ')}">
        ${!isDevelopment ? `<link rel="stylesheet" type="text/css" href="${stylesUri}">` : ''}
        <title>BAML Playground</title>
      </head>
      <body>
        <div id="root"></div>
        <script type="module" ${isDevelopment ? '' : `nonce="${nonce}"`} src="${scriptUri}"></script>
      </body>
    </html>`;
}
