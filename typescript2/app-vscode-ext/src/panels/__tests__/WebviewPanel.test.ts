import { beforeEach, describe, expect, it, vi } from 'vitest';

// A port is part of a panel's identity: its port mapping and the WS URL
// baked into its HTML both name the server it was built for. After a
// server restart the port changes, and reusing the panel would leave it
// pointed at a process that is gone.

const { created } = vi.hoisted(() => {
  interface Created {
    portMapping: { webviewPort: number; extensionHostPort: number }[];
    disposed: boolean;
    revealed: number;
  }
  const created: Created[] = [];
  return { created };
});

vi.mock('vscode', () => {
  const makeUri = (fsPath: string) => ({
    fsPath,
    path: fsPath,
    toString: () => fsPath,
  });
  return {
    commands: { executeCommand: vi.fn() },
    Position: class {},
    Range: class {},
    Selection: class {},
    TextEditorRevealType: { InCenter: 1 },
    Uri: {
      file: makeUri,
      joinPath: (base: { fsPath: string }, ...paths: string[]) =>
        makeUri([base.fsPath.replace(/\/$/, ''), ...paths].join('/')),
    },
    ViewColumn: { Beside: -2, One: 1 },
    window: {
      activeTextEditor: undefined,
      createWebviewPanel: (
        _id: string,
        _title: string,
        _column: unknown,
        options: {
          portMapping: { webviewPort: number; extensionHostPort: number }[];
        },
      ) => {
        const record = {
          disposed: false,
          portMapping: options.portMapping,
          revealed: 0,
        };
        created.push(record);
        return {
          dispose: () => {
            record.disposed = true;
          },
          onDidDispose: () => ({ dispose() {} }),
          reveal: () => {
            record.revealed += 1;
          },
          webview: {
            asWebviewUri: (uri: unknown) => uri,
            cspSource: 'vscode-webview://test',
            html: '',
            onDidReceiveMessage: () => ({ dispose() {} }),
            postMessage: async () => true,
          },
        };
      },
      onDidChangeTextEditorSelection: () => ({ dispose() {} }),
      showErrorMessage: vi.fn(),
      showTextDocument: vi.fn(),
      visibleTextEditors: [],
    },
    workspace: { openTextDocument: vi.fn() },
  };
});

vi.mock('../getWebviewHtml', () => ({
  getPlaygroundHtml: async (_webview: unknown, _uri: unknown, port: number) =>
    `<html data-port="${port}"></html>`,
}));

import { Uri } from 'vscode';
import { WebviewPanel } from '../WebviewPanel';

const extensionUri = Uri.file('/extension');
const target = { project: '/proj' };

beforeEach(() => {
  WebviewPanel.currentPanel?.dispose();
  created.length = 0;
});

describe('WebviewPanel.render', () => {
  it('builds a panel mapped to the requested port', async () => {
    await WebviewPanel.render(extensionUri, 4265, target);

    expect(created).toHaveLength(1);
    expect(created[0].portMapping).toEqual([
      { extensionHostPort: 4265, webviewPort: 4265 },
    ]);
    expect(WebviewPanel.currentPanel?.port).toBe(4265);
  });

  it('reveals the existing panel for the same port', async () => {
    await WebviewPanel.render(extensionUri, 4265, target);
    await WebviewPanel.render(extensionUri, 4265, { project: '/other' });

    expect(created).toHaveLength(1);
    expect(created[0].revealed).toBe(1);
    expect(created[0].disposed).toBe(false);
  });

  it('replaces the panel when the port changes', async () => {
    await WebviewPanel.render(extensionUri, 4265, target);
    await WebviewPanel.render(extensionUri, 4266, target);

    expect(created).toHaveLength(2);
    expect(created[0].disposed).toBe(true);
    expect(created[1].portMapping).toEqual([
      { extensionHostPort: 4266, webviewPort: 4266 },
    ]);
    expect(WebviewPanel.currentPanel?.port).toBe(4266);
  });
});
