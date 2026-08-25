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

// ── One client per window ────────────────────────────────────────────────
//
// The server multiplexes projects: it receives the window's workspace
// folders, discovers every BAML project underneath them, and serves ALL
// `.baml` documents — including materialized stdlib sources, which belong
// to no project and previously caused the extension to spawn a doomed
// sibling server per stdlib directory (its `baml.openBamlPanel`
// registration collided with the first client's and start() rejected).
// One client also makes the `baml.trace.server` setting work (the client
// id is the literal `baml`), and Start/Stop/Restart act on the whole
// window instead of one arbitrary project.

let client: LanguageClient | undefined;
let clientStartFailed = false;
let knownProjects: string[] = [];
let currentServerState: 'starting' | 'running' | 'stopped' | 'error' = 'starting';
let statusBarItem: vscode.StatusBarItem | undefined;
let playgroundDir: string | undefined;
let wrapperPath = 'baml';

function getExtVersion(): string {
  return vscode.extensions.getExtension('Boundary.baml-language')?.packageJSON?.version ?? '?';
}

/** Short display name: last path component (e.g. "/Users/x/repos/myapp/baml_src" → "myapp/baml_src") */
function projectLabel(fullPath: string): string {
  const normalized = path.normalize(fullPath);
  const name = path.basename(normalized);
  const parent = path.basename(path.dirname(normalized));
  return parent && parent !== name ? `${parent}/${name}` : name || fullPath;
}

function buildStatusTooltip(serverState: 'starting' | 'running' | 'stopped' | 'error'): vscode.MarkdownString {
  const serverVersion = client?.initializeResult?.serverInfo?.version ?? '—';

  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = true;
  md.supportThemeIcons = true;

  md.appendMarkdown(`Extension Info: Version ${getExtVersion()}, Server Version ${serverVersion}\n\n`);
  md.appendMarkdown(`---\n\n`);
  md.appendMarkdown(`[$(output) Open Logs](command:baml.openLogs)\n\n`);

  const projects = [...knownProjects].sort();
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

function playgroundArgsForPath(projectPath?: string): { args: string[]; cwd?: string } {
  if (!projectPath) {
    return { args: ['playground'] };
  }

  try {
    const stat = fs.statSync(projectPath);
    if (stat.isFile()) {
      return {
        args: ['playground', '--file', projectPath],
        cwd: path.dirname(projectPath),
      };
    }
    if (stat.isDirectory()) {
      return {
        args: ['playground', '--from', projectPath],
        cwd: projectPath,
      };
    }
  } catch {
    // Fall through to --from. The CLI will surface the real path error.
  }

  return { args: ['playground', '--from', projectPath] };
}

function openPlaygroundInBrowserTerminal(projectPath?: string): void {
  const { args, cwd } = playgroundArgsForPath(projectPath);
  // The CLI IS the terminal's process (no shell): a shell-hosted server gets
  // killed about a second in when the Python extension reclaims the new
  // terminal to type its venv activation (it interrupts the foreground
  // process first). No shell also means no quoting and no shell integration.
  const terminal = vscode.window.createTerminal({
    name: 'BAML Playground',
    shellPath: wrapperPath,
    shellArgs: args,
    ...(cwd ? { cwd } : {}),
  });
  terminal.show(false);
}

function createClient(context: vscode.ExtensionContext): LanguageClient {
  const firstFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const serverOptions: ServerOptions = {
    command: wrapperPath,
    args: ['lsp'],
    options: {
      ...(firstFolder ? { cwd: firstFolder } : {}),
      env: {
        ...process.env,
        ...(playgroundDir ? { BAML_PLAYGROUND_DIR: playgroundDir } : {}),
      },
    },
  };

  const clientOptions: LanguageClientOptions = {
    // Every `.baml` document in the window — project files, files outside
    // any project (the server mints provisional roots for those), and
    // materialized stdlib sources alike. No `workspaceFolder` pin: the
    // client library forwards ALL workspace folders in `initialize` and
    // `workspace/didChangeWorkspaceFolders`, and the server discovers
    // projects underneath them.
    documentSelector: [{ language: 'baml', scheme: 'file' }],
    synchronize: {
      fileEvents: [
        vscode.workspace.createFileSystemWatcher('**/*.baml'),
        // Marker churn (baml.toml / baml_src appearing or vanishing)
        // reaches the server as didChangeWatchedFiles; it rediscovers.
        vscode.workspace.createFileSystemWatcher('**/{baml.toml,baml_src}'),
      ],
    },
    initializationOptions: {
      bamlClient: {
        kind: 'vscode',
        extensionVersion: getExtVersion(),
        supportedLspProtocol: { min: BAML_LSP_PROTOCOL_MIN, max: BAML_LSP_PROTOCOL_MAX },
        supportedPlaygroundProtocol: { min: BAML_PLAYGROUND_PROTOCOL_MIN, max: BAML_PLAYGROUND_PROTOCOL_MAX },
        capabilities: ['openPlayground.v1', 'listProjects.v1', 'playgroundWebSocket.v1'],
      },
    },
  };

  // The literal id `baml` is what makes the `baml.trace.server` setting
  // work: vscode-languageclient looks the trace level up under the client
  // id, and the previous per-root ids (`baml:/path`) never matched.
  const created = new LanguageClient('baml', 'BAML Language Server', serverOptions, clientOptions);

  created.onDidChangeState((e) => {
    switch (e.newState) {
      case State.Starting:
        updateStatusBar('starting');
        break;
      case State.Running:
        clientStartFailed = false;
        updateStatusBar('running');
        validateServerCompatibility(created);
        break;
      case State.Stopped:
        knownProjects = [];
        updateStatusBar(clientStartFailed ? 'error' : 'stopped');
        break;
    }
  });

  created.onNotification(
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

  created.onNotification('baml/listProjects', (params: { projects: string[] }) => {
    knownProjects = params.projects ?? [];
    refreshTooltip();
  });

  return created;
}

async function startClient(): Promise<void> {
  if (!client || client.state !== State.Stopped) {
    return;
  }
  try {
    await client.start();
    clientStartFailed = false;
  } catch (error) {
    clientStartFailed = true;
    updateStatusBar('error');
    console.error('Failed to start the BAML language server', error);
  }
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
  const config = vscode.workspace.getConfiguration('baml');
  playgroundDir = getPlaygroundDir(context);
  wrapperPath = process.env.BAML_CLI_PATH ?? config.get<string | null>('cliPath') ?? 'baml';

  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 0);
  statusBarItem.text = '$(loading~spin) 🐑 BAML';
  statusBarItem.tooltip = buildStatusTooltip('starting');
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  client = createClient(context);

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
      if (client && client.state !== State.Stopped) {
        await client.stop();
        vscode.window.showInformationMessage('BAML Language Server stopped.');
      }
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.startLanguageServer', async () => {
      await startClient();
      if (client?.state === State.Running) {
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

  context.subscriptions.push(
    vscode.commands.registerCommand('baml.openPlaygroundInBrowser', (projectPath?: string) => {
      openPlaygroundInBrowserTerminal(projectPath);
    }),
  );

  // Activation fires on `onLanguage:baml`, so a BAML document exists (or
  // is about to): start immediately.
  await startClient();
}

export async function deactivate() {
  const current = client;
  client = undefined;
  if (current && current.state !== State.Stopped) {
    await current.stop();
  }
}
