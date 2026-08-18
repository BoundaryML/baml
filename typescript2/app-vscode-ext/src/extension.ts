import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
  State,
} from 'vscode-languageclient/node';
import {
  BAML_LSP_PROTOCOL_MAX,
  BAML_LSP_PROTOCOL_MIN,
  BAML_PLAYGROUND_PROTOCOL_MAX,
  BAML_PLAYGROUND_PROTOCOL_MIN,
  type BamlServerMetadata,
  isProtocolCompatible,
} from './compat';
import { WebviewPanel } from './panels/WebviewPanel';
import {
  playgroundCommandForPath,
  shellForDefaultWindowsProfile,
  type WindowsTerminalProfile,
} from './playground-command';
import {
  type BamlProjectRoots,
  type CanonicalPath,
  canonicalPathIdentity,
  type RoutableOwnershipPattern,
  resolveBamlProjectRoots,
  resolveOwnershipRoot,
  routableOwnershipPattern,
} from './projectRoots';

const clients = new Map<string, LanguageClient>();
const clientFileWatchers = new Map<string, vscode.FileSystemWatcher[]>();
const clientRouteSignatures = new Map<string, string>();
/** Owners whose most recent start attempt failed. Sticky until a successful
 *  start so the aggregate status can stay "error" when every start failed. */
const failedClientStarts = new Set<string>();
const knownProjects = new Map<string, string[]>();
let currentServerState: 'starting' | 'running' | 'stopped' | 'error' =
  'starting';
let statusBarItem: vscode.StatusBarItem | undefined;
let extensionContext: vscode.ExtensionContext | undefined;
let ownershipCoordinator: OwnershipCoordinator | undefined;
let playgroundDir: string | undefined;
let wrapperPath = 'baml';

function getExtVersion(): string {
  return (
    vscode.extensions.getExtension('Boundary.baml-language')?.packageJSON
      ?.version ?? '?'
  );
}

/** Short display name: last path component (e.g. "/Users/x/repos/myapp/baml_src" → "myapp/baml_src") */
function projectLabel(fullPath: string): string {
  const normalized = path.normalize(fullPath);
  const name = path.basename(normalized);
  const parent = path.basename(path.dirname(normalized));
  return parent && parent !== name ? `${parent}/${name}` : name || fullPath;
}

function buildStatusTooltip(
  serverState: 'starting' | 'running' | 'stopped' | 'error',
): vscode.MarkdownString {
  const serverVersion =
    activeClient()?.initializeResult?.serverInfo?.version ?? '—';

  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = true;
  md.supportThemeIcons = true;

  md.appendMarkdown(
    `Extension Info: Version ${getExtVersion()}, Server Version ${serverVersion}\n\n`,
  );
  md.appendMarkdown('---\n\n');
  md.appendMarkdown('[$(output) Open Logs](command:baml.openLogs)\n\n');

  const projects = Array.from(
    new Set(Array.from(knownProjects.values()).flat()),
  ).sort();
  if (projects.length > 0) {
    for (const project of projects) {
      const encoded = encodeURIComponent(JSON.stringify(project));
      md.appendMarkdown(
        `[$(play) Open Playground — ${projectLabel(project)}](command:baml.openPlayground?${encoded})\n\n`,
      );
    }
  } else {
    md.appendMarkdown(
      '[$(play) Open Playground](command:baml.openPlayground)\n\n',
    );
  }

  md.appendMarkdown('---\n\n');

  if (serverState === 'running') {
    md.appendMarkdown(
      '[$(debug-stop) Stop Server](command:baml.stopLanguageServer)\n\n',
    );
    md.appendMarkdown(
      '[$(debug-restart) Restart Server](command:baml.restartLanguageServer)\n\n',
    );
  } else if (serverState === 'stopped' || serverState === 'error') {
    md.appendMarkdown(
      '[$(debug-start) Start Server](command:baml.startLanguageServer)\n\n',
    );
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

function getPlaygroundDir(
  context: vscode.ExtensionContext,
): string | undefined {
  const playgroundDir = vscode.Uri.joinPath(
    context.extensionUri,
    'dist',
    'playground',
  ).fsPath;
  return fs.existsSync(playgroundDir) ? playgroundDir : undefined;
}

function activeDocumentUri(): vscode.Uri | undefined {
  const editor = vscode.window.activeTextEditor;
  if (
    editor?.document.languageId === 'baml' &&
    editor.document.uri.scheme === 'file'
  ) {
    return editor.document.uri;
  }
  return vscode.workspace.textDocuments.find(
    (doc) => doc.languageId === 'baml' && doc.uri.scheme === 'file',
  )?.uri;
}

/**
 * Documents in an unmarked directory tree (no `baml.toml`/`baml_src`) still
 * get a server: their workspace folder (or containing directory) acts as the
 * ownership root, matching the extension's historical behavior.
 */
function fallbackOwnershipRoot(uri: vscode.Uri): CanonicalPath {
  const workspaceRoot = vscode.workspace.getWorkspaceFolder(uri)?.uri.fsPath;
  return canonicalPathIdentity(workspaceRoot ?? path.dirname(uri.fsPath));
}

function ownershipRootForUri(uri: vscode.Uri): CanonicalPath {
  return resolveOwnershipRoot(uri.fsPath, 'file') ?? fallbackOwnershipRoot(uri);
}

function activeClient(): LanguageClient | undefined {
  const uri = activeDocumentUri();
  if (!uri) {
    return clients.values().next().value;
  }
  const ownerKey =
    ownershipCoordinator?.ownerKeyForUri(uri) ?? ownershipRootForUri(uri).key;
  return clients.get(ownerKey);
}

function projectKeyForPath(projectPath: string): string | undefined {
  try {
    return (
      resolveOwnershipRoot(projectPath, 'auto') ??
      fallbackOwnershipRoot(vscode.Uri.file(projectPath))
    ).key;
  } catch {
    return undefined;
  }
}

function defaultTerminalShell(): string {
  if (process.platform !== 'win32') {
    return process.platform;
  }
  const terminalConfig = vscode.workspace.getConfiguration(
    'terminal.integrated',
  );
  return shellForDefaultWindowsProfile(
    terminalConfig.get<string | null>('defaultProfile.windows'),
    terminalConfig.get<Record<string, WindowsTerminalProfile | null>>(
      'profiles.windows',
    ),
  );
}

function openPlaygroundInBrowserTerminal(projectPath?: string): void {
  const { command, cwd } = playgroundCommandForPath({
    platform: process.platform,
    projectPath,
    shell: defaultTerminalShell(),
    wrapperPath,
  });
  const terminal = vscode.window.createTerminal({
    name: 'BAML Playground',
    ...(cwd ? { cwd } : {}),
  });
  terminal.show(false);
  terminal.sendText(command);
}

async function ensureClient(
  ownershipRoot: CanonicalPath,
  routablePatterns: readonly RoutableOwnershipPattern[],
): Promise<LanguageClient> {
  const ownerKey = ownershipRoot.key;
  const projectRoot = ownershipRoot.fsPath;
  const canonicalPattern: RoutableOwnershipPattern = {
    basePath: projectRoot,
    pattern: '**/*.baml',
  };
  const patternByKey = new Map<string, RoutableOwnershipPattern>();
  for (const route of [canonicalPattern, ...routablePatterns]) {
    const normalized = {
      basePath: path.normalize(route.basePath),
      pattern: route.pattern,
    };
    patternByKey.set(
      `${normalized.basePath}\u0000${normalized.pattern}`,
      normalized,
    );
  }
  const routePatterns = [...patternByKey.values()].sort((left, right) =>
    `${left.basePath}\u0000${left.pattern}`.localeCompare(
      `${right.basePath}\u0000${right.pattern}`,
    ),
  );
  const routeSignature = routePatterns
    .map((route) => `${route.basePath}\u0000${route.pattern}`)
    .join('\u0001');
  let existing = clients.get(ownerKey);
  if (existing && clientRouteSignatures.get(ownerKey) !== routeSignature) {
    // Keep one process for the canonical owner while refreshing the immutable
    // LanguageClient selector to include a newly observed symlink spelling.
    await removeClient(ownerKey);
    existing = undefined;
  }
  if (existing) {
    if (existing.state === State.Stopped) {
      try {
        await existing.start();
        failedClientStarts.delete(ownerKey);
      } catch (error) {
        failedClientStarts.add(ownerKey);
        updateStatusBar('error');
        throw error;
      }
    }
    return existing;
  }
  const serverOptions: ServerOptions = {
    args: ['lsp'],
    command: wrapperPath,
    options: {
      cwd: projectRoot,
      env: {
        ...process.env,
        ...(playgroundDir ? { BAML_PLAYGROUND_DIR: playgroundDir } : {}),
      },
    },
  };

  const ownerFolder: vscode.WorkspaceFolder = {
    index: 0,
    name: path.basename(projectRoot),
    uri: vscode.Uri.file(projectRoot),
  };
  const bamlFiles = routePatterns.map(
    ({ basePath, pattern }) =>
      new vscode.RelativePattern(
        basePath === projectRoot ? ownerFolder : basePath,
        pattern,
      ),
  );
  const fileWatchers = [
    ...bamlFiles.map((pattern) =>
      vscode.workspace.createFileSystemWatcher(pattern),
    ),
    vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(ownerFolder, WORKSPACE_MARKER_GLOB),
    ),
  ];
  // vscode-languageclient 9 types this as the protocol selector (whose
  // pattern is string-only), while its runtime passes the selector directly
  // to vscode.languages.match, which supports RelativePattern.
  const documentSelector = bamlFiles.map((pattern) => ({
    language: 'baml',
    pattern,
    scheme: 'file',
  })) satisfies vscode.DocumentSelector;
  const clientOptions: LanguageClientOptions = {
    documentSelector:
      documentSelector as unknown as LanguageClientOptions['documentSelector'],
    initializationOptions: {
      bamlClient: {
        capabilities: [
          'openPlayground.v1',
          'listProjects.v1',
          'playgroundWebSocket.v1',
        ],
        extensionVersion: getExtVersion(),
        kind: 'vscode',
        projectRoot,
        supportedLspProtocol: {
          max: BAML_LSP_PROTOCOL_MAX,
          min: BAML_LSP_PROTOCOL_MIN,
        },
        supportedPlaygroundProtocol: {
          max: BAML_PLAYGROUND_PROTOCOL_MAX,
          min: BAML_PLAYGROUND_PROTOCOL_MIN,
        },
      },
    },
    synchronize: {
      // Marker events must reach retained clients as didChangeWatchedFiles;
      // the coordinator's topology watchers separately decide ownership.
      fileEvents: fileWatchers,
    },
    workspaceFolder: ownerFolder,
  };

  const client = new LanguageClient(
    `baml:${ownerKey}`,
    `BAML Language Server (${projectLabel(projectRoot)})`,
    serverOptions,
    clientOptions,
  );
  clients.set(ownerKey, client);
  clientFileWatchers.set(ownerKey, fileWatchers);
  clientRouteSignatures.set(ownerKey, routeSignature);
  wireClient(ownerKey, client);
  try {
    await client.start();
    failedClientStarts.delete(ownerKey);
    return client;
  } catch (error) {
    clients.delete(ownerKey);
    clientFileWatchers.delete(ownerKey);
    clientRouteSignatures.delete(ownerKey);
    for (const watcher of fileWatchers) watcher.dispose();
    failedClientStarts.add(ownerKey);
    updateStatusBar('error');
    throw error;
  }
}

async function removeClient(ownerKey: string): Promise<void> {
  const client = clients.get(ownerKey);
  if (!client) return;

  // Remove command routing before waiting for shutdown. A replacement owner is
  // only started after this promise resolves.
  clients.delete(ownerKey);
  knownProjects.delete(ownerKey);
  try {
    if (client.state !== State.Stopped) {
      await client.stop();
    }
  } finally {
    knownProjects.delete(ownerKey);
    for (const watcher of clientFileWatchers.get(ownerKey) ?? []) {
      watcher.dispose();
    }
    clientFileWatchers.delete(ownerKey);
    clientRouteSignatures.delete(ownerKey);
    refreshTooltip();
  }
}

function updateAggregateServerState(): void {
  const states = Array.from(clients.values(), (client) => client.state);
  if (states.some((state) => state === State.Running)) {
    updateStatusBar('running');
  } else if (states.some((state) => state === State.Starting)) {
    updateStatusBar('starting');
  } else if (failedClientStarts.size > 0) {
    // Every start attempt failed: stay in "error" so the user sees the
    // failure instead of a neutral "stopped" from the final recompute.
    updateStatusBar('error');
  } else {
    updateStatusBar('stopped');
  }
}

function wireClient(ownerKey: string, client: LanguageClient) {
  if (!extensionContext) {
    return;
  }
  const context = extensionContext;
  client.onDidChangeState((e) => {
    switch (e.newState) {
      case State.Starting:
        updateAggregateServerState();
        break;
      case State.Running:
        updateAggregateServerState();
        validateServerCompatibility(client);
        break;
      case State.Stopped:
        knownProjects.delete(ownerKey);
        updateAggregateServerState();
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
        ...(params.functionName !== undefined
          ? { functionName: params.functionName }
          : {}),
        ...(params.testName !== undefined ? { testName: params.testName } : {}),
        ...(params.testsetName !== undefined
          ? { testsetName: params.testsetName }
          : {}),
      });
    },
  );

  client.onNotification(
    'baml/listProjects',
    (params: { projects: string[] }) => {
      knownProjects.set(ownerKey, params.projects ?? []);
      refreshTooltip();
    },
  );
}

function validateServerCompatibility(client: LanguageClient) {
  const metadata = client.initializeResult?.capabilities?.experimental?.baml as
    | BamlServerMetadata
    | undefined;
  if (!metadata?.lspProtocol || !metadata.minSupportedClientLspProtocol) {
    return;
  }
  if (
    !isProtocolCompatible(
      metadata.lspProtocol,
      metadata.minSupportedClientLspProtocol,
      {
        max: BAML_LSP_PROTOCOL_MAX,
        min: BAML_LSP_PROTOCOL_MIN,
      },
    )
  ) {
    vscode.window.showWarningMessage(
      'BAML language server protocol is incompatible with this extension. Update the BAML extension or the active BAML toolchain.',
    );
  }
}

interface TrackedDocument {
  uri: vscode.Uri;
  resolution: BamlProjectRoots | undefined;
  ancestorKeys: Set<string>;
}

interface RefCountedTopologyWatcher {
  refCount: number;
  watcher: vscode.FileSystemWatcher;
  subscriptions: vscode.Disposable[];
}

interface WorkspaceTopologyWatcher {
  watcher: vscode.FileSystemWatcher;
  subscriptions: vscode.Disposable[];
}

const EXACT_MARKER_GLOB = '{baml.toml,baml_src}';
const WORKSPACE_MARKER_GLOB = '**/{baml.toml,baml_src}';

/**
 * Owns document-to-client routing and marker topology. Language clients only
 * watch BAML files inside their non-overlapping owner; marker changes are
 * observed once here and migrated in a serialized lane.
 *
 * Clients are NOT stopped when their last document closes — a running server
 * may still serve a playground webview. A client is only removed when marker
 * topology reassigns its documents to a different owner (or on shutdown).
 */
class OwnershipCoordinator implements vscode.Disposable {
  private readonly documents = new Map<string, TrackedDocument>();
  private readonly documentOwners = new Map<string, string>();
  private readonly ancestorWatchers = new Map<
    string,
    RefCountedTopologyWatcher
  >();
  private readonly workspaceWatchers = new Map<
    string,
    WorkspaceTopologyWatcher
  >();
  private queue: Promise<void> = Promise.resolve();
  private disposed = false;

  constructor() {
    this.syncWorkspaceWatchers();
  }

  ownerKeyForUri(uri: vscode.Uri): string | undefined {
    return this.documentOwners.get(uri.toString());
  }

  trackDocument(uri: vscode.Uri): Promise<void> {
    return this.trackDocuments([uri]);
  }

  trackDocuments(uris: readonly vscode.Uri[]): Promise<void> {
    return this.enqueue(async () => {
      for (const uri of uris) {
        if (uri.scheme !== 'file') continue;
        const documentKey = uri.toString();
        let tracked = this.documents.get(documentKey);
        if (tracked) {
          tracked.uri = uri;
        } else {
          tracked = {
            ancestorKeys: new Set(),
            resolution: undefined,
            uri,
          };
          this.documents.set(documentKey, tracked);
        }
        this.refreshDocumentResolution(tracked);
      }
      await this.reconcileClients();
    });
  }

  untrackDocument(uri: vscode.Uri): Promise<void> {
    return this.enqueue(async () => {
      const documentKey = uri.toString();
      const tracked = this.documents.get(documentKey);
      if (!tracked) return;

      // Routing for the closed document goes away, but its owner's client is
      // deliberately retained (see class docs): stopping it here would kill
      // the playground webview that server may be serving.
      this.documentOwners.delete(documentKey);
      for (const ancestorKey of tracked.ancestorKeys) {
        this.releaseAncestorWatcher(ancestorKey);
      }
      this.documents.delete(documentKey);
      await this.reconcileClients();
    });
  }

  topologyChanged(): Promise<void> {
    return this.enqueue(async () => {
      await this.reconcile();
    });
  }

  workspaceFoldersChanged(): Promise<void> {
    return this.enqueue(async () => {
      this.syncWorkspaceWatchers();
      await this.reconcile();
    });
  }

  private enqueue(operation: () => Promise<void>): Promise<void> {
    this.queue = this.queue
      .then(async () => {
        if (!this.disposed) await operation();
      })
      .catch((error: unknown) => {
        console.error('BAML ownership migration failed', error);
        updateStatusBar('error');
      });
    return this.queue;
  }

  private async reconcile(): Promise<void> {
    for (const tracked of this.documents.values()) {
      this.refreshDocumentResolution(tracked);
    }
    await this.reconcileClients();
  }

  private refreshDocumentResolution(tracked: TrackedDocument): void {
    const resolution = resolveBamlProjectRoots(tracked.uri.fsPath, 'file');
    const desiredAncestors = new Map(
      resolution.ancestors.map((ancestor) => [ancestor.key, ancestor] as const),
    );

    // Install new observers before releasing obsolete ones so symlink topology
    // changes cannot leave a gap in marker coverage.
    for (const [ancestorKey, ancestor] of desiredAncestors) {
      if (!tracked.ancestorKeys.has(ancestorKey)) {
        this.retainAncestorWatcher(ancestor);
      }
    }
    for (const ancestorKey of tracked.ancestorKeys) {
      if (!desiredAncestors.has(ancestorKey)) {
        this.releaseAncestorWatcher(ancestorKey);
      }
    }

    tracked.resolution = resolution;
    tracked.ancestorKeys = new Set(desiredAncestors.keys());
  }

  private async reconcileClients(): Promise<void> {
    const desiredRoots = new Map<
      string,
      {
        owner: CanonicalPath;
        routablePatterns: Map<string, RoutableOwnershipPattern>;
      }
    >();
    const desiredRoutes = new Map<string, string>();

    for (const [documentKey, tracked] of this.documents) {
      // Unmarked documents fall back to their workspace folder so a plain
      // .baml file still gets language services.
      const owner =
        tracked.resolution?.ownershipRoot ?? fallbackOwnershipRoot(tracked.uri);
      const desired = desiredRoots.get(owner.key) ?? {
        owner,
        routablePatterns: new Map<string, RoutableOwnershipPattern>(),
      };
      const route = tracked.resolution
        ? routableOwnershipPattern(tracked.uri.fsPath, tracked.resolution)
        : undefined;
      if (route) {
        desired.routablePatterns.set(
          `${path.normalize(route.basePath)}\u0000${route.pattern}`,
          route,
        );
      }
      desiredRoots.set(owner.key, desired);
      desiredRoutes.set(documentKey, owner.key);
    }

    // A client is removed only when ownership genuinely migrated: one of the
    // documents it used to own is still tracked but now resolves to a
    // different owner (marker topology changed). Owners that merely lost
    // their last tracked document keep running — their server may be serving
    // a playground webview.
    const migratedFrom = new Set<string>();
    for (const [documentKey, newOwnerKey] of desiredRoutes) {
      const previousOwner = this.documentOwners.get(documentKey);
      if (previousOwner && previousOwner !== newOwnerKey) {
        migratedFrom.add(previousOwner);
      }
    }

    // Routing is detached before any old process is stopped. Since replacement
    // processes start only after all obsolete owners stop, a document is never
    // selected by both clients during marker migration.
    this.documentOwners.clear();
    for (const ownerKey of Array.from(clients.keys())) {
      if (!desiredRoots.has(ownerKey) && migratedFrom.has(ownerKey)) {
        await removeClient(ownerKey);
      }
    }

    for (const { owner, routablePatterns } of desiredRoots.values()) {
      try {
        await ensureClient(owner, [...routablePatterns.values()]);
      } catch (error) {
        console.error(
          `Failed to start BAML language server for ${owner.fsPath}`,
          error,
        );
      }
    }

    for (const [documentKey, ownerKey] of desiredRoutes) {
      if (clients.has(ownerKey)) {
        this.documentOwners.set(documentKey, ownerKey);
      }
    }
    updateAggregateServerState();
  }

  private retainAncestorWatcher(directory: CanonicalPath): void {
    const existing = this.ancestorWatchers.get(directory.key);
    if (existing) {
      existing.refCount += 1;
      return;
    }

    const watcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(directory.fsPath, EXACT_MARKER_GLOB),
    );
    this.ancestorWatchers.set(directory.key, {
      refCount: 1,
      subscriptions: this.listenForTopologyChanges(watcher),
      watcher,
    });
  }

  private releaseAncestorWatcher(directoryKey: string): void {
    const entry = this.ancestorWatchers.get(directoryKey);
    if (!entry) return;
    entry.refCount -= 1;
    if (entry.refCount > 0) return;

    this.disposeWatcher(entry);
    this.ancestorWatchers.delete(directoryKey);
  }

  private syncWorkspaceWatchers(): void {
    const folders = vscode.workspace.workspaceFolders ?? [];
    const desired = new Map(
      folders.map((folder) => [folder.uri.toString(), folder] as const),
    );

    for (const [folderKey, entry] of this.workspaceWatchers) {
      if (!desired.has(folderKey)) {
        this.disposeWatcher(entry);
        this.workspaceWatchers.delete(folderKey);
      }
    }

    for (const [folderKey, folder] of desired) {
      if (this.workspaceWatchers.has(folderKey)) continue;
      const watcher = vscode.workspace.createFileSystemWatcher(
        new vscode.RelativePattern(folder, WORKSPACE_MARKER_GLOB),
      );
      this.workspaceWatchers.set(folderKey, {
        subscriptions: this.listenForTopologyChanges(watcher),
        watcher,
      });
    }
  }

  private listenForTopologyChanges(
    watcher: vscode.FileSystemWatcher,
  ): vscode.Disposable[] {
    const onTopologyChange = () => {
      void this.topologyChanged();
    };
    return [
      watcher.onDidCreate(onTopologyChange),
      watcher.onDidDelete(onTopologyChange),
      watcher.onDidChange(onTopologyChange),
    ];
  }

  private disposeWatcher(entry: WorkspaceTopologyWatcher): void {
    for (const subscription of entry.subscriptions) subscription.dispose();
    entry.watcher.dispose();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.documentOwners.clear();
    for (const entry of this.ancestorWatchers.values())
      this.disposeWatcher(entry);
    for (const entry of this.workspaceWatchers.values())
      this.disposeWatcher(entry);
    this.ancestorWatchers.clear();
    this.workspaceWatchers.clear();
    this.documents.clear();
  }

  async shutdown(): Promise<void> {
    this.dispose();
    await this.queue;
    for (const ownerKey of Array.from(clients.keys())) {
      await removeClient(ownerKey);
    }
  }
}

export async function activate(context: vscode.ExtensionContext) {
  extensionContext = context;
  const config = vscode.workspace.getConfiguration('baml');
  playgroundDir = getPlaygroundDir(context);
  wrapperPath =
    process.env.BAML_CLI_PATH ?? config.get<string | null>('cliPath') ?? 'baml';

  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    0,
  );
  statusBarItem.text = '$(loading~spin) 🐑 BAML';
  statusBarItem.tooltip = buildStatusTooltip('starting');
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  const coordinator = new OwnershipCoordinator();
  ownershipCoordinator = coordinator;
  context.subscriptions.push(coordinator);

  const startForUri = async (uri: vscode.Uri | undefined) => {
    if (uri) {
      await coordinator.trackDocument(uri);
    }
  };
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      if (
        editor?.document.languageId === 'baml' &&
        editor.document.uri.scheme === 'file'
      ) {
        void startForUri(editor.document.uri);
      }
    }),
  );
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (document.languageId === 'baml' && document.uri.scheme === 'file') {
        void startForUri(document.uri);
      }
    }),
  );
  context.subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((document) => {
      if (document.languageId === 'baml' && document.uri.scheme === 'file') {
        void coordinator.untrackDocument(document.uri);
      }
    }),
  );
  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      void coordinator.workspaceFoldersChanged();
    }),
  );

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
    vscode.commands.registerCommand(
      'baml.openPlayground',
      async (projectPath?: string) => {
        const projectOwnerKey = projectPath
          ? projectKeyForPath(projectPath)
          : undefined;
        const client = projectPath
          ? ((projectOwnerKey !== undefined
              ? clients.get(projectOwnerKey)
              : undefined) ?? activeClient())
          : activeClient();
        if (!client || client.state !== State.Running) {
          vscode.window.showWarningMessage(
            'BAML Language Server is not running.',
          );
          return;
        }
        const args: Record<string, unknown> = {};
        if (projectPath) {
          args.projectPath = projectPath;
        }
        await client.sendRequest('workspace/executeCommand', {
          arguments: [args],
          command: 'baml.openBamlPanel',
        });
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'baml.openPlaygroundInBrowser',
      (projectPath?: string) => {
        openPlaygroundInBrowserTerminal(projectPath);
      },
    ),
  );

  const openBamlDocuments = vscode.workspace.textDocuments.filter(
    (document) =>
      document.languageId === 'baml' && document.uri.scheme === 'file',
  );
  await coordinator.trackDocuments(
    openBamlDocuments.map((document) => document.uri),
  );
}

export async function deactivate() {
  const coordinator = ownershipCoordinator;
  ownershipCoordinator = undefined;
  if (coordinator) {
    await coordinator.shutdown();
  } else {
    for (const ownerKey of Array.from(clients.keys())) {
      await removeClient(ownerKey);
    }
  }
  extensionContext = undefined;
}
