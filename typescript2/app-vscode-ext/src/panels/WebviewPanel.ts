import {
  type Disposable,
  Uri,
  ViewColumn,
  type WebviewPanel as VSCodeWebviewPanel,
  window,
} from 'vscode';
import { getPlaygroundHtml } from './getWebviewHtml';

export class WebviewPanel {
  public static currentPanel: WebviewPanel | undefined;
  private readonly _panel: VSCodeWebviewPanel;
  private _disposables: Disposable[] = [];

  private constructor(panel: VSCodeWebviewPanel) {
    this._panel = panel;

    // Dispose listener
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
  }

  public static async render(extensionUri: Uri, port: number) {
    if (WebviewPanel.currentPanel) {
      WebviewPanel.currentPanel._panel.reveal(ViewColumn.Beside, true);
      return;
    }

    const panel = window.createWebviewPanel(
      'bamlPlayground',
      'BAML Playground',
      { viewColumn: ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        // Map the playground server port so scripts/WS can reach it.
        portMapping: [{ webviewPort: port, extensionHostPort: port }],
      }
    );

    WebviewPanel.currentPanel = new WebviewPanel(panel);

    // Show a loading message while we fetch the real HTML from the server.
    panel.webview.html = `<!DOCTYPE html>
<html><body style="display:flex;align-items:center;justify-content:center;height:100vh;color:#888;font-family:sans-serif;">
<p>Loading playground\u2026</p>
</body></html>`;

    try {
      panel.webview.html = await getPlaygroundHtml(port);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      panel.webview.html = `<!DOCTYPE html>
<html><body style="display:flex;align-items:center;justify-content:center;height:100vh;color:#c44;font-family:sans-serif;">
<p>Failed to load playground: ${msg}</p>
</body></html>`;
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
}
