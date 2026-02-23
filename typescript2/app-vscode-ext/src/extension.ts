import * as vscode from 'vscode';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  State,
} from 'vscode-languageclient/node';
import { WebviewPanel } from './panels/WebviewPanel';

let client: LanguageClient | undefined;
let knownProjects: string[] = [];
let currentServerState: 'starting' | 'running' | 'stopped' | 'error' = 'starting';
let statusBarItem: vscode.StatusBarItem | undefined;

function getExtVersion(): string {
  return vscode.extensions.getExtension('Boundary.app-vscode-ext')?.packageJSON?.version ?? '?';
}

/** Short display name: last path component (e.g. "/Users/x/repos/myapp/baml_src" → "myapp/baml_src") */
function projectLabel(fullPath: string): string {
  const parts = fullPath.replace(/\/$/, '').split('/');
  return parts.length >= 2 ? parts.slice(-2).join('/') : parts[parts.length - 1] ?? fullPath;
}

function buildStatusTooltip(serverState: 'starting' | 'running' | 'stopped' | 'error'): vscode.MarkdownString {
  const serverVersion = client?.initializeResult?.serverInfo?.version ?? '—';

  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = true;
  md.supportThemeIcons = true;

  md.appendMarkdown(`Extension Info: Version ${getExtVersion()}, Server Version ${serverVersion}\n\n`);
  md.appendMarkdown(`---\n\n`);
  md.appendMarkdown(`[$(output) Open Logs](command:baml.openLogs)\n\n`);

  if (knownProjects.length > 0) {
    for (const project of knownProjects) {
      const encoded = encodeURIComponent(JSON.stringify(project));
      md.appendMarkdown(`[$(play) Open Playground — ${projectLabel(project)}](command:baml.openPlayground?${encoded})\n\n`);
    }
  } else {
    md.appendMarkdown(`[$(play) Open Playground](command:baml.openPlayground)\n\n`);
  }

  md.appendMarkdown(`---\n\n`);

  if (serverState === 'running') {
    md.appendMarkdown(`[$(debug-stop) Stop Server](command:baml.stopLanguageServer)\n\n`);
    md.appendMarkdown(`[$(debug-restart) Restart Server](command:baml.restartLanguageServer)\n\n`);
  } else if (serverState === 'stopped' || serverState === 'error') {
    md.appendMarkdown(`[$(debug-start) Start Server](command:baml.startLanguageServer)\n\n`);
  }

  return md;
}

function updateStatusBar(state: 'starting' | 'running' | 'stopped' | 'error') {
  currentServerState = state;
  if (!statusBarItem) return;
  switch (state) {
    case 'starting':
      statusBarItem.text = '$(loading~spin) 🐑 BAML';
      break;
    case 'running':
      statusBarItem.text = '🐑 BAML';
      break;
    case 'stopped':
      statusBarItem.text = '$(circle-slash) 🐑 BAML';
      break;
    case 'error':
      statusBarItem.text = '$(error) 🐑 BAML';
      break;
  }
  statusBarItem.tooltip = buildStatusTooltip(state);
}

function refreshTooltip() {
  if (statusBarItem) {
    statusBarItem.tooltip = buildStatusTooltip(currentServerState);
  }
}

export async function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('baml');

  // Priority: BAML_CLI_PATH env var (for debug) → setting → PATH lookup
  const cliPath =
    process.env.BAML_CLI_PATH ??
    config.get<string | null>('cliPath') ??
    'baml-cli';

  const serverOptions: ServerOptions = {
    command: cliPath,
    args: ['lsp'],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: 'baml', scheme: 'file' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.baml'),
    },
  };

  client = new LanguageClient(
    'baml',
    'BAML Language Server',
    serverOptions,
    clientOptions,
  );

  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  statusBarItem.text = '$(loading~spin) 🐑 BAML';
  statusBarItem.tooltip = buildStatusTooltip('starting');
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  client.onDidChangeState((e) => {
    switch (e.newState) {
      case State.Starting:
        updateStatusBar('starting');
        break;
      case State.Running:
        updateStatusBar('running');
        break;
      case State.Stopped:
        knownProjects = [];
        updateStatusBar('stopped');
        break;
    }
  });

  // ── Commands ────────────────────────────────────────────────────────

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.openLogs', () => {
      client?.outputChannel.show();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.restartLanguageServer', async () => {
      if (client) {
        await client.restart();
        vscode.window.showInformationMessage('BAML Language Server restarted.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.stopLanguageServer', async () => {
      if (client) {
        await client.stop();
        vscode.window.showInformationMessage('BAML Language Server stopped.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.startLanguageServer', async () => {
      if (client) {
        await client.start();
        vscode.window.showInformationMessage('BAML Language Server started.');
      }
    }),
  );

  // "Open Playground" accepts an optional project path (passed from the
  // status bar tooltip links). Routes through the LSP so
  // NativePlaygroundSender can decide how to open it (and attach the port).
  context.subscriptions.push(
    vscode.commands.registerCommand('baml.openPlayground', async (projectPath?: string) => {
      if (!client || client.state !== State.Running) {
        vscode.window.showWarningMessage('BAML Language Server is not running.');
        return;
      }
      const args: Record<string, unknown> = {};
      if (projectPath) {
        args.projectPath = projectPath;
      }
      await client.sendRequest('workspace/executeCommand', {
        command: 'baml.openBamlPanel',
        arguments: [args],
      });
    }),
  );

  await client.start();

  // The LSP sends `baml/openPlayground` when the user clicks a code lens
  // or invokes the manual command above. The notification carries the port.
  client.onNotification(
    'baml/openPlayground',
    async (params: { port: number; projectPath: string; functionName?: string }) => {
      await WebviewPanel.render(context.extensionUri, params.port);
    },
  );

  // Track discovered projects so the status bar tooltip can show per-project links.
  client.onNotification(
    'baml/listProjects',
    (params: { projects: string[] }) => {
      knownProjects = params.projects ?? [];
      refreshTooltip();
    },
  );
}

export async function deactivate() {
  if (client) {
    await client.stop();
  }
}
