import {
  type Disposable,
  Uri,
  ViewColumn,
  type Webview,
  type WebviewPanel as VSCodeWebviewPanel,
  window,
} from 'vscode';
import { getNonce } from '../utils/getNonce';
import { getUri } from '../utils/getUri';

export class WebviewPanel {
  public static currentPanel: WebviewPanel | undefined;
  private readonly _panel: VSCodeWebviewPanel;
  private _disposables: Disposable[] = [];

  private constructor(panel: VSCodeWebviewPanel, extensionUri: Uri) {
    this._panel = panel;

    // Dispose listener
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    // Set the HTML content
    this._panel.webview.html = this._getWebviewContent(
      this._panel.webview,
      extensionUri
    );

    // Listen for messages from webview
    this._setWebviewMessageListener(this._panel.webview);
  }

  public static render(extensionUri: Uri) {
    if (WebviewPanel.currentPanel) {
      WebviewPanel.currentPanel._panel.reveal(ViewColumn.Beside, true);
    } else {
      const panel = window.createWebviewPanel(
        'bamlPlayground',
        'BAML Playground',
        { viewColumn: ViewColumn.Beside, preserveFocus: true },
        {
          enableScripts: true,
          localResourceRoots: [
            Uri.joinPath(extensionUri, 'dist'),
            Uri.joinPath(extensionUri, 'node_modules', 'app-vscode-webview', 'dist'),
          ],
          retainContextWhenHidden: true,
        }
      );

      WebviewPanel.currentPanel = new WebviewPanel(panel, extensionUri);
    }
  }

  public dispose() {
    WebviewPanel.currentPanel = undefined;
    this._panel.dispose();

    while (this._disposables.length) {
      const disposable = this._disposables.pop();
      if (disposable) {
        disposable.dispose();
      }
    }
  }

  private _getWebviewContent(webview: Webview, extensionUri: Uri): string {
    const isDevelopment = process.env.VSCODE_DEBUG_MODE === 'true';
    const devServerUrl = 'localhost:5173';

    const nonce = getNonce();

    let stylesUri: string;
    let scriptUri: string;

    if (isDevelopment) {
      // Load from Vite dev server
      stylesUri = `http://${devServerUrl}/src/index.css`;
      scriptUri = `http://${devServerUrl}/src/main.tsx`;
    } else {
      // Load from bundled assets in node_modules
      stylesUri = getUri(webview, extensionUri, [
        'node_modules',
        'app-vscode-webview',
        'dist',
        'assets',
        'index.css',
      ]).toString();
      scriptUri = getUri(webview, extensionUri, [
        'node_modules',
        'app-vscode-webview',
        'dist',
        'assets',
        'index.js',
      ]).toString();
    }

    const csp = [
      `default-src 'none'`,
      `script-src 'unsafe-eval' ${
        isDevelopment
          ? `http://${devServerUrl} 'nonce-${nonce}'`
          : `'nonce-${nonce}'`
      }`,
      `style-src ${webview.cspSource} 'self' 'unsafe-inline' ${
        isDevelopment ? `http://${devServerUrl}` : ''
      }`,
      `font-src ${webview.cspSource}`,
      `connect-src ${
        isDevelopment
          ? `ws://${devServerUrl} http://${devServerUrl}`
          : ''
      }`,
      `img-src ${webview.cspSource} https: data:`,
    ];

    const reactRefresh = isDevelopment
      ? `<script type="module" nonce="${nonce}">
          import RefreshRuntime from "http://${devServerUrl}/@react-refresh"
          RefreshRuntime.injectIntoGlobalHook(window)
          window.$RefreshReg$ = () => {}
          window.$RefreshSig$ = () => (type) => type
          window.__vite_plugin_react_preamble_installed__ = true
        </script>`
      : '';

    return `<!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <meta http-equiv="Content-Security-Policy" content="${csp.join('; ')}">
        <link rel="stylesheet" type="text/css" href="${stylesUri}">
        <title>BAML Playground</title>
      </head>
      <body>
        <div id="root"></div>
        ${reactRefresh}
        <script type="module" ${isDevelopment ? '' : `nonce="${nonce}"`} src="${scriptUri}"></script>
      </body>
    </html>`;
  }

  private _setWebviewMessageListener(webview: Webview) {
    webview.onDidReceiveMessage(
      (message: unknown) => {
        console.log('Message from webview:', message);
      },
      undefined,
      this._disposables
    );
  }
}
