import {
  type Disposable,
  Position,
  Range,
  Selection,
  TextEditorRevealType,
  Uri,
  ViewColumn,
  type WebviewPanel as VSCodeWebviewPanel,
  window,
  workspace,
} from 'vscode';
import { getPlaygroundHtml } from './getWebviewHtml';

type SourceNavigationTarget = {
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
} & (
  | { filePath: string; fileId?: number }
  | { fileId: number; filePath?: string }
);

export class WebviewPanel {
  public static currentPanel: WebviewPanel | undefined;
  private readonly _panel: VSCodeWebviewPanel;
  private _disposables: Disposable[] = [];

  private constructor(panel: VSCodeWebviewPanel) {
    this._panel = panel;

    // Dispose listener
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    this._panel.webview.onDidReceiveMessage(
      (message: { type?: string; source?: SourceNavigationTarget }) => {
        if (message.type !== 'navigateToSource' || !message.source) return;
        void this.navigateToSource(message.source);
      },
      null,
      this._disposables,
    );
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

  private async navigateToSource(source: SourceNavigationTarget): Promise<void> {
    if (!source.filePath) {
      void window.showErrorMessage(
        `Cannot navigate to source for file id ${source.fileId}: no file path was provided.`,
      );
      return;
    }

    const targetUri = Uri.file(source.filePath);

    const visibleEditor = window.visibleTextEditors.find(
      (editor) => editor.document.uri.toString() === targetUri.toString(),
    );
    const document = visibleEditor?.document ?? await workspace.openTextDocument(targetUri);
    const editor = await window.showTextDocument(document, {
      viewColumn: visibleEditor?.viewColumn ?? ViewColumn.One,
      preserveFocus: false,
      preview: false,
    });

    const start = new Position(Math.max(0, source.line - 1), source.column);
    const end = new Position(
      Math.max(0, (source.endLine ?? source.line) - 1),
      source.endColumn ?? source.column,
    );
    const range = new Range(start, end);
    editor.selection = new Selection(end, start);
    editor.revealRange(range, TextEditorRevealType.InCenter);
  }
}
