import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  State,
} from 'vscode-languageclient/node';
import { WebviewPanel } from './panels/WebviewPanel';
import {
  BAML_LSP_PROTOCOL_MAX,
  BAML_LSP_PROTOCOL_MIN,
  BAML_PLAYGROUND_PROTOCOL_MAX,
  BAML_PLAYGROUND_PROTOCOL_MIN,
  isProtocolCompatible,
  type BamlServerMetadata,
} from './compat';

const clients = new Map<string, LanguageClient>();
const knownProjects = new Map<string, string[]>();
let currentServerState: 'starting' | 'running' | 'stopped' | 'error' = 'starting';
let statusBarItem: vscode.StatusBarItem | undefined;
let extensionContext: vscode.ExtensionContext | undefined;
let playgroundDir: string | undefined;
let wrapperPath = 'baml';

function getExtVersion(): string {
  return vscode.extensions.getExtension('Boundary.baml-language')?.packageJSON?.version ?? '?';
}

/** Short display name: last path component (e.g. "/Users/x/repos/myapp/baml_src" → "myapp/baml_src") */
function projectLabel(fullPath: string): string {
  const parts = fullPath.replace(/\/$/, '').split('/');
  return parts.length >= 2 ? parts.slice(-2).join('/') : parts[parts.length - 1] ?? fullPath;
}

function buildStatusTooltip(serverState: 'starting' | 'running' | 'stopped' | 'error'): vscode.MarkdownString {
  const serverVersion = activeClient()?.initializeResult?.serverInfo?.version ?? '—';

  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = true;
  md.supportThemeIcons = true;

  md.appendMarkdown(`Extension Info: Version ${getExtVersion()}, Server Version ${serverVersion}\n\n`);
  md.appendMarkdown(`---\n\n`);
  md.appendMarkdown(`[$(output) Open Logs](command:baml.openLogs)\n\n`);

  const projects = Array.from(new Set(Array.from(knownProjects.values()).flat())).sort();
  if (projects.length > 0) {
    for (const project of projects) {
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

function getPlaygroundDir(context: vscode.ExtensionContext): string | undefined {
  const playgroundDir = vscode.Uri.joinPath(context.extensionUri, 'dist', 'playground').fsPath;
  return fs.existsSync(playgroundDir) ? playgroundDir : undefined;
}

function activeDocumentUri(): vscode.Uri | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor?.document.languageId === 'baml' && editor.document.uri.scheme === 'file') {
    return editor.document.uri;
  }
  return vscode.workspace.textDocuments.find((doc) => doc.languageId === 'baml' && doc.uri.scheme === 'file')?.uri;
}

function activeClient(): LanguageClient | undefined {
  const uri = activeDocumentUri();
  if (!uri) {
    return clients.values().next().value;
  }
  const root = findBamlProjectRoot(uri);
  return clients.get(root);
}

function findBamlProjectRoot(uri: vscode.Uri): string {
  let dir = fs.statSync(uri.fsPath).isDirectory() ? uri.fsPath : path.dirname(uri.fsPath);
  const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
  const workspaceRoot = workspaceFolder?.uri.fsPath;

  while (true) {
    if (fs.existsSync(path.join(dir, 'baml.toml'))) {
      return dir;
    }
    if (workspaceRoot && dir === workspaceRoot) {
      return workspaceRoot;
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      break;
    }
    dir = parent;
  }

  return workspaceRoot ?? path.dirname(uri.fsPath);
}

function projectKeyForPath(projectPath: string): string {
  try {
    return findBamlProjectRoot(vscode.Uri.file(projectPath));
  } catch {
    return projectPath;
  }
}

async function ensureClient(projectRoot: string): Promise<LanguageClient> {
  const existing = clients.get(projectRoot);
  if (existing) {
    if (existing.state === State.Stopped) {
      await existing.start();
    }
    return existing;
  }
  const serverOptions: ServerOptions = {
    command: wrapperPath,
    args: ['lsp'],
    options: {
      cwd: projectRoot,
      env: {
        ...process.env,
        ...(playgroundDir ? { BAML_PLAYGROUND_DIR: playgroundDir } : {}),
      },
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: 'baml', scheme: 'file' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.baml'),
    },
    initializationOptions: {
      bamlClient: {
        kind: 'vscode',
        extensionVersion: getExtVersion(),
        projectRoot,
        supportedLspProtocol: { min: BAML_LSP_PROTOCOL_MIN, max: BAML_LSP_PROTOCOL_MAX },
        supportedPlaygroundProtocol: { min: BAML_PLAYGROUND_PROTOCOL_MIN, max: BAML_PLAYGROUND_PROTOCOL_MAX },
        capabilities: ['openPlayground.v1', 'listProjects.v1', 'playgroundWebSocket.v1'],
      },
    },
  };

  const client = new LanguageClient(
    `baml:${projectRoot}`,
    `BAML Language Server (${projectLabel(projectRoot)})`,
    serverOptions,
    clientOptions,
  );
  clients.set(projectRoot, client);
  wireClient(projectRoot, client);
  await client.start();
  return client;
}

function wireClient(projectRoot: string, client: LanguageClient) {
  if (!extensionContext) {
    return;
  }
  const context = extensionContext;
  client.onDidChangeState((e) => {
    switch (e.newState) {
      case State.Starting:
        updateStatusBar('starting');
        break;
      case State.Running:
        updateStatusBar('running');
        validateServerCompatibility(client);
        break;
      case State.Stopped:
        knownProjects.delete(projectRoot);
        updateStatusBar('stopped');
        break;
    }
  });

  client.onNotification(
    'baml/openPlayground',
    async (params: {
      port: number;
      projectPath: string;
      functionName?: string;
      testName?: string;
      testsetName?: string;
    }) => {
      await WebviewPanel.render(context.extensionUri, params.port, {
        project: params.projectPath,
        ...(params.functionName !== undefined ? { functionName: params.functionName } : {}),
        ...(params.testName !== undefined ? { testName: params.testName } : {}),
        ...(params.testsetName !== undefined ? { testsetName: params.testsetName } : {}),
      });
    },
  );

  client.onNotification(
    'baml/listProjects',
    (params: { projects: string[] }) => {
      knownProjects.set(projectRoot, params.projects ?? []);
      refreshTooltip();
    },
  );
}

function validateServerCompatibility(client: LanguageClient) {
  const metadata = client.initializeResult?.capabilities?.experimental?.baml as BamlServerMetadata | undefined;
  if (!metadata?.lspProtocol || !metadata.minSupportedClientLspProtocol) {
    return;
  }
  if (!isProtocolCompatible(metadata.lspProtocol, metadata.minSupportedClientLspProtocol, {
    min: BAML_LSP_PROTOCOL_MIN,
    max: BAML_LSP_PROTOCOL_MAX,
  })) {
    vscode.window.showWarningMessage('BAML language server protocol is incompatible with this extension. Update the BAML extension or the active BAML toolchain.');
  }
}

export async function activate(context: vscode.ExtensionContext) {
  extensionContext = context;
  const config = vscode.workspace.getConfiguration('baml');
  playgroundDir = getPlaygroundDir(context);
  wrapperPath = process.env.BAML_CLI_PATH ?? config.get<string | null>('cliPath') ?? 'baml';

  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  statusBarItem.text = '$(loading~spin) 🐑 BAML';
  statusBarItem.tooltip = buildStatusTooltip('starting');
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  const startForUri = async (uri: vscode.Uri | undefined) => {
    if (uri) {
      await ensureClient(findBamlProjectRoot(uri));
    }
  };
  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor((editor) => {
    if (editor?.document.languageId === 'baml') {
      void startForUri(editor.document.uri);
    }
  }));
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument((document) => {
    if (document.languageId === 'baml' && document.uri.scheme === 'file') {
      void startForUri(document.uri);
    }
  }));

  // ── Commands ────────────────────────────────────────────────────────

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.openLogs', () => {
      activeClient()?.outputChannel.show();
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.restartLanguageServer', async () => {
      const client = activeClient();
      if (client) {
        await client.restart();
        vscode.window.showInformationMessage('BAML Language Server restarted.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.stopLanguageServer', async () => {
      const client = activeClient();
      if (client) {
        await client.stop();
        vscode.window.showInformationMessage('BAML Language Server stopped.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.startLanguageServer', async () => {
      const client = activeClient();
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
      const client = projectPath
        ? clients.get(projectKeyForPath(projectPath)) ?? activeClient()
        : activeClient();
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

  await startForUri(activeDocumentUri());
}

export async function deactivate() {
  for (const client of clients.values()) {
    await client.stop();
  }
}
