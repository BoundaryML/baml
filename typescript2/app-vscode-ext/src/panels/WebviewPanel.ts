import {
  type Disposable,
  Position,
  Range,
  Selection,
  type TextEditor,
  type TextEditorSelectionChangeEvent,
  TextEditorRevealType,
  Uri,
  ViewColumn,
  type WebviewPanel as VSCodeWebviewPanel,
  commands,
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

interface CursorPosition {
  file: string;
  line: number;
  column: number;
}

interface OpenPlaygroundTarget {
  project: string;
  functionName?: string;
  testName?: string;
  testsetName?: string;
}

function isBamlEditor(editor: TextEditor): boolean {
  return editor.document.languageId === 'baml' || editor.document.uri.fsPath.endsWith('.baml');
}

export class WebviewPanel {
  public static currentPanel: WebviewPanel | undefined;
  private readonly _panel: VSCodeWebviewPanel;
  private _disposables: Disposable[] = [];
  private _openTarget: OpenPlaygroundTarget;

  private constructor(panel: VSCodeWebviewPanel, openTarget: OpenPlaygroundTarget) {
    this._panel = panel;
    this._openTarget = openTarget;

    // Dispose listener
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);
    this._panel.webview.onDidReceiveMessage(
      (message: { type?: string; source?: SourceNavigationTarget; project?: string }) => {
        if (message.type === 'webviewReady') {
          this.forwardOpenPlayground();
          this.forwardActiveEditorCursorPosition();
          return;
        }
        if (message.type === 'navigateToSource' && message.source) {
          void this.navigateToSource(message.source);
        } else if (message.type === 'openInBrowser') {
          void commands.executeCommand(
            'baml.openPlaygroundInBrowser',
            message.project ?? this._openTarget.project,
          );
        }
      },
      null,
      this._disposables,
    );

    const selectionDisposable = window.onDidChangeTextEditorSelection((event) => {
      this.forwardCursorPosition(event);
    });
    this._disposables.push(selectionDisposable);
    this.forwardActiveEditorCursorPosition();
  }

  public static async render(extensionUri: Uri, port: number, openTarget: OpenPlaygroundTarget) {
    if (WebviewPanel.currentPanel) {
      WebviewPanel.currentPanel._openTarget = openTarget;
      WebviewPanel.currentPanel._panel.reveal(ViewColumn.Beside, true);
      WebviewPanel.currentPanel.forwardOpenPlayground();
      WebviewPanel.currentPanel.forwardActiveEditorCursorPosition();
      return;
    }

    const panel = window.createWebviewPanel(
      'bamlPlayground',
      'BAML Playground',
      { viewColumn: ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        localResourceRoots: [
          Uri.joinPath(extensionUri, 'dist', 'playground'),
        ],
        retainContextWhenHidden: true,
        // Map the playground server port so scripts/WS can reach it.
        portMapping: [{ webviewPort: port, extensionHostPort: port }],
      }
    );

    WebviewPanel.currentPanel = new WebviewPanel(panel, openTarget);

    // Show a loading message while we load the packaged playground shell.
    panel.webview.html = `<!DOCTYPE html>
<html><body style="display:flex;align-items:center;justify-content:center;height:100vh;color:#888;font-family:sans-serif;">
<p>Loading playground\u2026</p>
</body></html>`;

    try {
      panel.webview.html = await getPlaygroundHtml(panel.webview, extensionUri, port);
      WebviewPanel.currentPanel.forwardOpenPlayground();
      WebviewPanel.currentPanel.forwardActiveEditorCursorPosition();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      panel.webview.html = `<!DOCTYPE html>
<html><body style="display:flex;align-items:center;justify-content:center;height:100vh;color:#c44;font-family:sans-serif;">
<p>Failed to load playground: ${msg}</p>
</body></html>`;
    }
  }

  private forwardOpenPlayground(): void {
    void this._panel.webview.postMessage({
      type: 'openPlayground',
      target: this._openTarget,
    });
  }

  private forwardCursorPosition(event: TextEditorSelectionChangeEvent): void {
    this.forwardEditorCursorPosition(event.textEditor);
  }

  private forwardActiveEditorCursorPosition(): void {
    const editor = window.activeTextEditor;
    if (editor) {
      this.forwardEditorCursorPosition(editor);
    }
  }

  private forwardEditorCursorPosition(editor: TextEditor): void {
    if (!isBamlEditor(editor)) return;

    const active = editor.selection.active;
    const position: CursorPosition = {
      file: editor.document.uri.fsPath,
      line: active.line,
      column: active.character,
    };
    void this._panel.webview.postMessage({ type: 'cursorPosition', position });
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
