/**
 * MonacoEditor — BAML editor with file tree (explorer) and LSP.
 *
 * Initialization is split into two phases so the editor is visible immediately:
 *
 *   Workbench (fast): Init VS Code API wrapper + create editor with text.
 *                     The user sees the editor with their code right away.
 *
 *   Backend (async): Connect the injected {@link EditorBackend} — start the
 *                    language client and open the Playground pane. Language
 *                    features (hover, diagnostics, completions) light up once
 *                    the backend is connected.
 *
 * The backend is the only thing that differs between deployments:
 *   - app-promptfiddle injects a WASM-worker backend (LSP + runtime in-browser).
 *   - baml-cli playground injects a remote backend (LSP + runtime over a
 *     WebSocket to a real server). See `@b/pkg-editor/remote`.
 */

import { useEffect, useRef, useState, type FC } from 'react';
import './views-workbench.css';
import { type IFileWriteOptions } from '@codingame/monaco-vscode-files-service-override';
import type { Dimension } from '@codingame/monaco-vscode-api/vscode/vs/base/browser/dom';
import { isMediaPath, mimeFromPath, toDataUrl, fromDataUrl } from './media';
import type { EditorBackend, EditorConnection, WorkbenchHandle } from './backend';

declare const __DEV__: boolean | undefined;
declare const process: { env: { NODE_ENV?: string } } | undefined;

function isDevelopmentBuild(): boolean {
  if (typeof __DEV__ !== 'undefined') {
    return __DEV__;
  }
  return typeof process !== 'undefined' && process.env.NODE_ENV === 'development';
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MonacoEditorProps {
  /**
   * Unified file map: filename → content.
   * Text files (.baml, .toml, .json) have raw content strings.
   * Media files (.png, .jpg, etc.) have data-URL strings.
   */
  files: Record<string, string>;
  /** Called whenever any file changes, is created, or deleted. */
  onFilesChange: (files: Record<string, string>) => void;
  /** Provides the LSP transport + runtime (worker or remote). */
  backend: EditorBackend;
  /**
   * Absolute path the in-memory workspace is rooted at. Defaults to
   * '/workspace' (synthetic, for the WASM worker). For a remote server that
   * operates on real on-disk files, pass the real project root so the editor's
   * file URIs (`file://<root>/<rel>`) match the URIs the server emits in
   * diagnostics — no URI translation required.
   */
  workspaceRoot?: string;
  /** CSS height for the container. Defaults to '100%'. */
  height?: string;
  /** Notified with the latest map of media filename → blob: URL. */
  onBlobUrlsChange?: (urls: Record<string, string>) => void;
  /**
   * If set, enables VS Code's `afterDelay` auto-save with this debounce (ms),
   * so edits persist without an explicit Cmd+S. The remote backend uses this so
   * edits flow to the server (and through to disk) automatically. Omit to keep
   * manual-save semantics (the worker backend persists via onFilesChange).
   */
  autoSaveDelayMs?: number;
  /**
   * Track unsaved-edit state (for save-on-disk backends). When enabled, the
   * editor watches edit/save events and reports whether any file has edits not
   * yet written to disk via {@link onUnsavedChange}. (monaco's own dirty-dot
   * isn't reliably surfaced in this workbench's layout, so the host renders the
   * indicator itself — e.g. in a toolbar.)
   */
  showSaveHint?: boolean;
  /** Called with true/false as the unsaved-edits state changes (needs showSaveHint). */
  onUnsavedChange?: (hasUnsaved: boolean) => void;
}

function createWorkspaceContent(workspacePath: string): string {
  return JSON.stringify({ folders: [{ path: workspacePath }] }, null, 2);
}

// ---------------------------------------------------------------------------
// Loading skeleton — matches "Default Dark Modern" so the transition is smooth.
// Token colors are inline because they're hardcoded to the pre-workbench theme;
// structural layout uses Tailwind.
// ---------------------------------------------------------------------------

const sk = {
  bg: '#1f1f1f', sidebar: '#181818', sidebarBorder: '#2b2b2b',
  lineNum: '#6e7681', text: '#9da5b4', keyword: '#569cd6',
  string: '#ce9178', comment: '#6a9955',
} as const;

const SkeletonLine: FC<{ indent?: number; tokens: Array<{ w: number; color: string }> }> = ({ indent = 0, tokens }) => (
  <div className="flex items-center h-[21px]" style={{ paddingLeft: indent * 16 }}>
    {tokens.map((t, i) => (
      <div key={i} className="h-2.5 rounded-sm opacity-35 mr-2" style={{ width: t.w, background: t.color }} />
    ))}
  </div>
);

const EditorSkeleton: FC<{ height: string }> = ({ height }) => (
  <div className="w-full flex font-mono overflow-hidden bg-[#1f1f1f]" style={{ height }}>
    {/* Sidebar skeleton */}
    <div className="w-[200px] shrink-0 py-2.5 bg-[#181818] border-r border-[#2b2b2b]">
      <div className="px-3 mb-2.5">
        <div className="w-20 h-[9px] rounded-sm opacity-20 bg-[#9da5b4]" />
      </div>
      {[90, 70, 110, 60].map((w, i) => (
        <div key={i} className="py-0.5 px-3 pl-5">
          <div className="h-[9px] rounded-sm opacity-15 bg-[#9da5b4]" style={{ width: w }} />
        </div>
      ))}
    </div>

    {/* Editor skeleton */}
    <div className="flex-1 flex min-w-0">
      {/* Gutter */}
      <div className="w-12 shrink-0 pt-3 bg-[#1f1f1f]">
        {Array.from({ length: 12 }, (_, i) => (
          <div key={i} className="h-[21px] flex items-center justify-end pr-3">
            <div className="w-2.5 h-2 rounded-sm opacity-25 bg-[#6e7681]" />
          </div>
        ))}
      </div>

      {/* Code area */}
      <div className="flex-1 pt-3 pl-2">
        <SkeletonLine tokens={[{ w: 48, color: sk.comment }]} />
        <SkeletonLine tokens={[{ w: 55, color: sk.keyword }, { w: 80, color: sk.text }]} />
        <SkeletonLine indent={1} tokens={[{ w: 45, color: sk.keyword }, { w: 60, color: sk.text }]} />
        <SkeletonLine indent={1} tokens={[{ w: 50, color: sk.keyword }, { w: 90, color: sk.string }]} />
        <SkeletonLine tokens={[{ w: 10, color: sk.text }]} />
        <SkeletonLine tokens={[]} />
        <SkeletonLine tokens={[{ w: 42, color: sk.keyword }, { w: 70, color: sk.text }]} />
        <SkeletonLine indent={1} tokens={[{ w: 60, color: sk.keyword }, { w: 50, color: sk.text }]} />
        <SkeletonLine indent={1} tokens={[{ w: 55, color: sk.string }, { w: 80, color: sk.string }]} />
        <SkeletonLine indent={1} tokens={[{ w: 40, color: sk.keyword }, { w: 100, color: sk.string }]} />
        <SkeletonLine tokens={[{ w: 10, color: sk.text }]} />
        <SkeletonLine tokens={[]} />
      </div>
    </div>
  </div>
);

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MonacoEditor: FC<MonacoEditorProps> = ({ files, onFilesChange, backend, workspaceRoot = '/workspace', height = '100%', onBlobUrlsChange, autoSaveDelayMs, showSaveHint, onUnsavedChange }) => {
  /** Marks a file (by uri string) as saved/clean — set by the save-hint setup so
   *  the disk-change handler can clear externally-applied edits from the hint. */
  const markFileSavedRef = useRef<(uriString: string) => void>(() => {});
  /** Set of file uris with edits not yet saved to disk. */
  const unsavedFilesRef = useRef<Set<string>>(new Set());
  const onUnsavedChangeRef = useRef(onUnsavedChange);
  onUnsavedChangeRef.current = onUnsavedChange;
  const containerRef = useRef<HTMLDivElement>(null);
  const onFilesChangeRef = useRef(onFilesChange);
  const onBlobUrlsChangeRef = useRef(onBlobUrlsChange);
  const blobUrlsRef = useRef<Record<string, string>>({});
  const backendRef = useRef(backend);
  const filesRef = useRef(files);
  const [ready, setReady] = useState(false);
  const [connectionCount, setConnectionCount] = useState(0);
  const [mounted, setMounted] = useState(false);
  /** Active backend connection (LSP transport + runtime). */
  const currentConnRef = useRef<EditorConnection | null>(null);
  /** Disposables tied to the current connection (language client + connection). */
  const connDisposablesRef = useRef<Array<{ dispose: () => void | Promise<void> }>>([]);
  /** Increments each time we connect; used as React key to force ExecutionPanel remount. */
  const connectionVersionRef = useRef(0);
  /** Callback to restart the connection; set once `connect` is defined. */
  const restartRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    setMounted(true);
  }, []);

  onFilesChangeRef.current = onFilesChange;
  onBlobUrlsChangeRef.current = onBlobUrlsChange;
  backendRef.current = backend;
  filesRef.current = files;

  useEffect(() => {
    if (!containerRef.current) return;

    let disposed = false;
    const disposables: Array<{ dispose: () => void }> = [];

    (async () => {
      // ════════════════════════════════════════════════════════════════
      // Workbench — Show the editor with text ASAP
      // ════════════════════════════════════════════════════════════════

      // Parallel-import: VS Code API shim + service overrides together
      const [
        { MonacoVscodeApiWrapper, defaultHtmlAugmentationInstructions, defaultViewsInit },
        { createDefaultLocaleConfiguration },
        { useWorkerFactory, Worker: WorkerRef },
        keybindingsOverride,
        lifecycleOverride,
        localizationOverride,
        explorerOverride,
        filesOverride,
        bannerOverride,
        statusBarOverride,
        titleBarOverride,
        environmentOverride,
        remoteAgentOverride,
        searchOverride,
        outlineOverride,
        secretStorageOverride,
        storageOverride,
        vscode,
        { default: bamlTmLanguageGrammar },
      ] = await Promise.all([
        import('monaco-languageclient/vscodeApiWrapper'),
        import('monaco-languageclient/vscodeApiLocales'),
        import('monaco-languageclient/workerFactory'),
        import('@codingame/monaco-vscode-keybindings-service-override'),
        import('@codingame/monaco-vscode-lifecycle-service-override'),
        import('@codingame/monaco-vscode-localization-service-override'),
        import('@codingame/monaco-vscode-explorer-service-override'),
        import('@codingame/monaco-vscode-files-service-override'),
        import('@codingame/monaco-vscode-view-banner-service-override'),
        import('@codingame/monaco-vscode-view-status-bar-service-override'),
        import('@codingame/monaco-vscode-view-title-bar-service-override'),
        import('@codingame/monaco-vscode-environment-service-override'),
        import('@codingame/monaco-vscode-remote-agent-service-override'),
        import('@codingame/monaco-vscode-search-service-override'),
        import('@codingame/monaco-vscode-outline-service-override'),
        import('@codingame/monaco-vscode-secret-storage-service-override'),
        import('@codingame/monaco-vscode-storage-service-override'),
        import('vscode'),
        import('@b/pkg-grammar/baml.tmLanguage.json'),
      ]);

      if (disposed || !containerRef.current) return;

      // Workspace root (absolute path the in-memory FS is rooted at).
      const root = (workspaceRoot.replace(/\/+$/, '') || '') || '/workspace';
      const rootPrefix = `${root}/`;
      const workspaceConfigPath = `${root}.code-workspace`;

      // Set up in-memory filesystem
      const workspaceFolderUri = vscode.Uri.file(root);
      const workspaceFileUri = vscode.Uri.file(workspaceConfigPath);

      const { InMemoryFileSystemProvider, registerFileSystemOverlay, FileChangeType } = filesOverride;
      const rawFs = new InMemoryFileSystemProvider();
      const encoder = new TextEncoder();
      const decoder = new TextDecoder();
      const writeOpts: IFileWriteOptions = { atomic: false, unlock: false, create: true, overwrite: true };

      // Sandbox: only allow operations inside the workspace root (plus its config file).
      const WORKSPACE_ROOT = root;
      const WORKSPACE_CONFIG = workspaceConfigPath;

      /** Returns true if the path is inside /workspace or is the workspace config file. */
      const isAllowedPath = (uri: { path: string }): boolean => {
        const p = uri.path;
        return p === WORKSPACE_ROOT || p.startsWith(WORKSPACE_ROOT + '/') || p === WORKSPACE_CONFIG;
      };

      /** Throws if the path is outside the sandbox. */
      const assertAllowed = (uri: { path: string }, op: string): void => {
        if (!isAllowedPath(uri)) {
          throw new Error(`Sandbox violation: ${op} not allowed outside ${WORKSPACE_ROOT} (got ${uri.path})`);
        }
      };

      /** Throws if trying to delete/rename the workspace root itself. */
      const assertNotRoot = (uri: { path: string }, op: string): void => {
        if (uri.path === WORKSPACE_ROOT) {
          throw new Error(`Sandbox violation: cannot ${op} the workspace root directory`);
        }
      };

      // Wrap the raw FS provider with sandbox checks via Proxy.
      const fileSystemProvider = new Proxy(rawFs, {
        get(target, prop, receiver) {
          const val = Reflect.get(target, prop, receiver);
          if (typeof val !== 'function') return val;

          switch (prop) {
            case 'writeFile': return (uri: any, content: any, opts: any) => {
              assertAllowed(uri, 'writeFile');
              return target.writeFile(uri, content, opts);
            };
            case 'mkdir': return (uri: any) => {
              assertAllowed(uri, 'mkdir');
              return target.mkdir(uri);
            };
            case 'delete': return (uri: any, opts: any) => {
              assertAllowed(uri, 'delete');
              assertNotRoot(uri, 'delete');
              return target.delete(uri, opts);
            };
            case 'rename': return (from: any, to: any, opts: any) => {
              assertAllowed(from, 'rename (source)');
              assertNotRoot(from, 'rename');
              assertAllowed(to, 'rename (target)');
              return target.rename(from, to, opts);
            };
            default:
              return val.bind(target);
          }
        },
      });

      // Create the workspace directory and ALL its ancestors. When rooted at a
      // real on-disk path (e.g. /Users/me/project/baml_src), the ancestors
      // (/Users, /Users/me, …) live OUTSIDE the sandbox, so we must use the raw
      // (un-proxied) provider — the sandbox proxy only permits paths inside the
      // root and would reject creating the chain that leads to it.
      {
        const rootParts = root.split('/');
        for (let i = 2; i <= rootParts.length; i++) {
          const dir = rootParts.slice(0, i).join('/');
          if (!dir) continue;
          try { await rawFs.mkdir(vscode.Uri.file(dir)); } catch { /* already exists */ }
        }
      }

      // Write ALL persisted files to the in-memory FS.
      // Media files (images, etc.) are decoded from data URLs and also get blob URLs.
      const allFiles = filesRef.current;
      const blobUrlMap: Record<string, string> = {};
      blobUrlsRef.current = blobUrlMap;

      for (const [filename, content] of Object.entries(allFiles)) {
        const absPath = filename.startsWith(rootPrefix) ? filename : `${rootPrefix}${filename}`;
        const parts = absPath.split('/');
        for (let i = 2; i < parts.length; i++) {
          const parentPath = parts.slice(0, i).join('/');
          try {
            await fileSystemProvider.mkdir(vscode.Uri.file(parentPath));
          } catch { /* already exists */ }
        }

        if (isMediaPath(filename)) {
          const bytes = fromDataUrl(content);
          await fileSystemProvider.writeFile(vscode.Uri.file(absPath), bytes, writeOpts);
          const mime = mimeFromPath(filename);
          blobUrlMap[filename] = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mime }));
        } else {
          await fileSystemProvider.writeFile(vscode.Uri.file(absPath), encoder.encode(content), writeOpts);
        }
      }
      onBlobUrlsChangeRef.current?.({ ...blobUrlMap });

      // Write workspace config
      await fileSystemProvider.writeFile(
        workspaceFileUri,
        encoder.encode(createWorkspaceContent(root)),
        writeOpts,
      );
      registerFileSystemOverlay(1, fileSystemProvider);

      const windowLabel = backendRef.current.windowLabel ?? 'BAML Playground';

      // Init VS Code API wrapper and start the workbench
      const apiWrapper = new MonacoVscodeApiWrapper({
        $type: 'extended',
        viewsConfig: {
          $type: 'ViewsService',
          htmlContainer: containerRef.current,
          htmlAugmentationInstructions: defaultHtmlAugmentationInstructions,
          viewsInitFunc: defaultViewsInit,
        },
        workspaceConfig: {
          enableWorkspaceTrust: true,
          windowIndicator: { label: windowLabel, tooltip: '', command: '' },
          workspaceProvider: {
            trusted: true,
            async open() { return true; },
            workspace: { workspaceUri: workspaceFileUri },
          },
          configurationDefaults: {
            'window.title': 'BAML Playground${separator}${dirty}${activeEditorShort}',
          },
          productConfiguration: {
            nameShort: 'BAML Playground',
            nameLong: 'BAML Playground',
          },
        },
        serviceOverrides: {
          ...keybindingsOverride.default(),
          ...lifecycleOverride.default(),
          ...localizationOverride.default(createDefaultLocaleConfiguration()),
          ...bannerOverride.default(),
          ...statusBarOverride.default(),
          ...titleBarOverride.default(),
          ...explorerOverride.default(),
          ...remoteAgentOverride.default(),
          ...environmentOverride.default(),
          ...secretStorageOverride.default(),
          ...storageOverride.default(),
          ...searchOverride.default(),
          ...outlineOverride.default(),
        },
        monacoWorkerFactory: () => {
          // Custom worker factory — the `new URL(..., import.meta.url)` patterns
          // must be in OUR source code (the consuming app transpiles pkg-editor)
          // so the bundler can resolve them at build time into proper asset URLs.
          // eslint-disable-next-line react-hooks/rules-of-hooks -- not a React hook
          useWorkerFactory({
            workerLoaders: {
              editorWorkerService: () => new WorkerRef(
                new URL('@codingame/monaco-vscode-editor-api/esm/vs/editor/editor.worker.js', import.meta.url),
                { type: 'module' },
              ),
              TextMateWorker: () => new WorkerRef(
                new URL('@codingame/monaco-vscode-textmate-service-override/worker', import.meta.url),
                { type: 'module' },
              ),
            },
          });
        },
        userConfiguration: {
          json: JSON.stringify({
            'workbench.colorTheme': 'Default Dark Modern',
            'window.commandCenter': false,
            'workbench.layoutControl.enabled': false,
            'editor.wordBasedSuggestions': 'off',
            'editor.minimap.enabled': false,
            'editor.scrollBeyondLastLine': false,
            'editor.fontSize': 13,
            'editor.lineHeight': 1.6,
            'editor.tabSize': 2,
            'editor.renderLineHighlight': 'line',
            'editor.padding.top': 12,
            ...(autoSaveDelayMs != null
              ? { 'files.autoSave': 'afterDelay', 'files.autoSaveDelay': autoSaveDelayMs }
              : {}),
          }),
        },
        extensions: [{
          config: {
            name: 'baml-playground',
            publisher: 'boundaryml',
            version: '1.0.0',
            engines: { vscode: '*' },
            contributes: {
              commands: [
                { command: 'baml.openPlayground', title: 'BAML: Open Playground' },
                { command: 'baml.previewImage', title: 'BAML: Preview Image' },
              ],
              languages: [{
                id: 'baml',
                extensions: ['.baml'],
                aliases: ['BAML', 'baml'],
                configuration: './language-configuration.json',
              }],
              grammars: [{
                language: 'baml',
                scopeName: 'source.baml',
                path: './baml.tmLanguage.json',
              }],
            },
          },
          filesOrContents: new Map<string, string | URL>([
            ['./baml.tmLanguage.json', JSON.stringify(bamlTmLanguageGrammar)],
            ['./language-configuration.json', JSON.stringify({
              comments: {
                lineComment: '//',
                blockComment: ['{//', '//}'],
              },
              brackets: [['{', '}'], ['[', ']'], ['(', ')']],
              autoClosingPairs: [
                ['{', '}'],
                ['[', ']'],
                ['(', ')'],
                { open: '"', close: '"' },
                ['#"', '"#'],
                ["'", "'"],
                ['{#', '}'],
                ['{//', '//}'],
              ],
              surroundingPairs: [
                ['{', '}'],
                ['[', ']'],
                ['(', ')'],
                ['"', '"'],
                ["'", "'"],
              ],
            })],
          ]),
        }],
      });

      await apiWrapper.start();
      if (disposed) return;

      // Register the ExecutionPanel as a custom editor pane in the workbench.
      // This must happen after start() so the workbench services are available.
      const { registerExecutionPanelPane } = await import('./ExecutionPanelPane');
      await registerExecutionPanelPane();

      // ── Image preview pane ─────────────────────────────────────────
      // IEditorResolverService is not wired into MonacoEditorService in
      // this monaco-vscode-api setup, so we decorate openEditor to route
      // image files to our SimpleEditorPane instead of the text editor.
      //
      // initialize() creates the container once; renderInput() is called
      // every time a new image is opened and loads content from liveFiles
      // (media files are stored as data URLs) or falls back to rawFs.
      {
        const { SimpleEditorPane, SimpleEditorInput, registerEditorPane, EditorInputCapabilities } =
          await import('@codingame/monaco-vscode-api/service-override/tools/views');
        const { StandaloneServices: SS } = await import('@codingame/monaco-vscode-api');
        const { IEditorService } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/workbench/services/editor/common/editorService.service'
        );

        const IMAGE_PANE_ID = 'baml.imagePreview';
        const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico']);
        const WORKSPACE_PREFIX = rootPrefix;

        class ImagePreviewInput extends SimpleEditorInput {
          constructor(uri: any) {
            super(uri);
            const name = String(uri.path ?? '').split('/').pop() ?? 'Image';
            this.setName(name);
            this.setTitle(name);
            this.addCapability(EditorInputCapabilities.Readonly);
          }
          get typeId() { return IMAGE_PANE_ID; }
          get editorId() { return IMAGE_PANE_ID; }
        }

        class ImagePreviewPane extends SimpleEditorPane {
          private _el: HTMLElement | null = null;
          private _img: HTMLImageElement | null = null;
          private _w = 0;
          private _h = 0;

          initialize(): HTMLElement {
            const el = document.createElement('div');
            el.style.overflow = 'hidden';
            el.style.background = '#1e1e1e';
            this._el = el;
            return el;
          }

          async renderInput(input: any): Promise<{ dispose: () => void }> {
            const el = this._el;
            if (!el) return { dispose() {} };

            el.innerHTML = '';
            this._img = null;

            const uri = input?.resource;
            if (!uri?.path) {
              el.textContent = 'No image to display';
              Object.assign(el.style, { color: '#ccc', padding: '2em' });
              return { dispose() {} };
            }

            const filename = String(uri.path).startsWith(WORKSPACE_PREFIX)
              ? String(uri.path).slice(WORKSPACE_PREFIX.length)
              : null;

            let dataUrl: string | undefined;
            if (filename) {
              dataUrl = liveFiles[filename];
            }

            if (!dataUrl) {
              try {
                const bytes: Uint8Array = await Promise.resolve(rawFs.readFile(uri));
                dataUrl = toDataUrl(bytes, mimeFromPath(String(uri.path)));
              } catch (err) {
                console.error('[ImagePreview] readFile failed:', err);
                el.textContent = `Failed to load image: ${err}`;
                Object.assign(el.style, { color: '#ccc', padding: '2em' });
                return { dispose() {} };
              }
            }

            const img = document.createElement('img');
            img.style.display = 'block';
            img.style.objectFit = 'contain';
            img.style.maxWidth = `${this._w}px`;
            img.style.maxHeight = `${this._h}px`;
            img.src = dataUrl;
            img.alt = String(uri.path).split('/').pop() ?? '';
            el.appendChild(img);
            this._img = img;

            return { dispose() { img.remove(); } };
          }

          layout(dimension: Dimension) {
            super.layout(dimension);
            this._w = dimension.width;
            this._h = dimension.height;
            if (this._el) {
              this._el.style.width = `${dimension.width}px`;
              this._el.style.height = `${dimension.height}px`;
            }
            if (this._img) {
              this._img.style.maxWidth = `${dimension.width}px`;
              this._img.style.maxHeight = `${dimension.height}px`;
            }
          }

          dispose() {
            this._el = null;
            this._img = null;
            super.dispose();
          }
        }

        registerEditorPane(IMAGE_PANE_ID, 'Image Preview', ImagePreviewPane as any, [ImagePreviewInput]);

        const editorService = SS.get(IEditorService);
        const origOpen = editorService.openEditor.bind(editorService);

        // @ts-expect-error override openEditor is expliclity desisred due to override
        editorService.openEditor = function (input: any, optionsOrGroup?: any, group?: any) {
          const resource = input?.resource ?? input?.original?.resource;
          const ext = resource?.path?.split('.')?.pop()?.toLowerCase() ?? '';
          if (resource && IMAGE_EXTS.has(ext)) {
            return origOpen(new ImagePreviewInput(resource), optionsOrGroup, group);
          }
          return origOpen(input, optionsOrGroup, group);
        };

        vscode.commands.registerCommand('baml.previewImage', (uri?: any) => {
          if (!uri) uri = vscode.window.activeTextEditor?.document.uri;
          if (!uri) return;
          editorService.openEditor(new ImagePreviewInput(uri));
        });
      }

      // Register the code block renderer for hover markdown.
      // Without this, MarkdownRendererService._defaultCodeBlockRenderer is undefined
      // and all code fences in hover widgets render as empty <span> elements.
      {
        const { StandaloneServices } = await import('@codingame/monaco-vscode-api');
        const { IMarkdownRendererService } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/platform/markdown/browser/markdownRenderer.service'
        );
        const { EditorMarkdownCodeBlockRenderer } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/editor/browser/widget/markdownRenderer/browser/editorMarkdownCodeBlockRenderer'
        );
        const { IConfigurationService } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/platform/configuration/common/configuration.service'
        );
        const { ILanguageService } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/editor/common/languages/language.service'
        );

        const markdownService = StandaloneServices.get(IMarkdownRendererService);
        const codeBlockRenderer = new EditorMarkdownCodeBlockRenderer(
          StandaloneServices.get(IConfigurationService),
          StandaloneServices.get(ILanguageService),
        );
        markdownService.setDefaultCodeBlockRenderer(codeBlockRenderer);
      }

      // Give the workbench a tick to finish restoring its session from IndexedDB.
      // Without this, the restored session can overwrite our showTextDocument call.
      await new Promise((r) => setTimeout(r, 150));
      if (disposed) return;

      // Close any stale editors restored from a previous session so we start clean.
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      if (disposed) return;

      // Determine which file to show — prefer main.baml, fall back to first text file
      const fileNames = Object.keys(allFiles).filter(f => !isMediaPath(f));
      const firstFileIndex = fileNames.findIndex(path => path.endsWith('main.baml'));
      const firstFile = firstFileIndex !== -1 ? fileNames[firstFileIndex] : fileNames[0];
      const firstFileUri = vscode.Uri.file(`${rootPrefix}${firstFile}`);

      // Open the document and show it in the editor
      await vscode.workspace.openTextDocument(firstFileUri);
      if (disposed) return;
      await vscode.window.showTextDocument(firstFileUri);
      if (disposed) return;

      // Focus Explorer so file tree shows
      vscode.commands.executeCommand('workbench.view.explorer').then(() => {}, () => {});

      // Workbench ready — editor is visible, hide skeleton
      setReady(true);

      // ── Unsaved-changes indicator ────────────────────────────────────
      // For save-on-disk backends, drive a React-rendered badge (below) while
      // there are edits not yet written to disk. We track this from edit/save
      // events — monaco's own dirty-dot isn't surfaced in this workbench's
      // layout (no visible status bar). Externally-applied edits (pushed from
      // disk) are cleared via markFileSavedRef so they don't read as "unsaved".
      if (showSaveHint) {
        const refresh = () => onUnsavedChangeRef.current?.(unsavedFilesRef.current.size > 0);
        const onChange = vscode.workspace.onDidChangeTextDocument((e) => {
          if (e.document.uri.path.endsWith('.baml')) {
            unsavedFilesRef.current.add(e.document.uri.toString());
            refresh();
          }
        });
        const onSave = vscode.workspace.onDidSaveTextDocument((doc) => {
          unsavedFilesRef.current.delete(doc.uri.toString());
          refresh();
        });
        markFileSavedRef.current = (uriString: string) => {
          unsavedFilesRef.current.delete(uriString);
          refresh();
        };
        disposables.push({
          dispose: () => {
            onChange.dispose();
            onSave.dispose();
          },
        });
      }

      // ── Track live file state ────────────────────────────────────────
      // Single mutable map for all files (text + media).
      // Text files store raw content, media files store data URLs.
      const liveFiles: Record<string, string> = { ...allFiles };

      /** Helper: extract the workspace-relative filename from a vscode Uri. */
      const uriToFilename = (uri: { path: string }): string | null => {
        if (uri.path.startsWith(rootPrefix)) {
          return uri.path.slice(rootPrefix.length);
        }
        return null;
      };

      /** Notify parent of the latest file state and push to the active backend. */
      const pushUpdate = () => {
        const snapshot = { ...liveFiles };
        onFilesChangeRef.current(snapshot);
        currentConnRef.current?.onFilesChanged?.(snapshot);
      };

      /** Create a blob URL for a media file and notify the host. */
      const updateBlobUrl = (filename: string, bytes: Uint8Array) => {
        if (blobUrlMap[filename]) {
          URL.revokeObjectURL(blobUrlMap[filename]);
        }
        const mime = mimeFromPath(filename);
        blobUrlMap[filename] = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mime }));
        onBlobUrlsChangeRef.current?.({ ...blobUrlMap });
      };

      /** Remove a blob URL for a deleted media file. */
      const removeBlobUrl = (filename: string) => {
        if (blobUrlMap[filename]) {
          URL.revokeObjectURL(blobUrlMap[filename]);
          delete blobUrlMap[filename];
          onBlobUrlsChangeRef.current?.({ ...blobUrlMap });
        }
      };

      // Listen for text changes from the editor (any .baml file)
      const changeSubscription = vscode.workspace.onDidChangeTextDocument((e) => {
        const filename = uriToFilename(e.document.uri);
        if (filename && filename.endsWith('.baml')) {
          liveFiles[filename] = e.document.getText();
          pushUpdate();
        }
      });
      disposables.push({ dispose: () => changeSubscription.dispose() });

      // Cursor position tracking for playground navigation
      let cursorDebounceTimer: ReturnType<typeof setTimeout> | undefined;
      const cursorSubscription = vscode.window.onDidChangeTextEditorSelection((e: import('vscode').TextEditorSelectionChangeEvent) => {
        const filename = uriToFilename(e.textEditor.document.uri);
        if (!filename || !filename.endsWith('.baml')) return;

        clearTimeout(cursorDebounceTimer);
        cursorDebounceTimer = setTimeout(() => {
          const pos = e.selections[0]?.active;
          if (!pos) return;
          // VS Code API positions are 0-indexed, matching lsp_types::Position.
          currentConnRef.current?.onCursorMoved?.(filename, pos.line, pos.character);
        }, 50);
      });
      disposables.push({ dispose: () => { clearTimeout(cursorDebounceTimer); cursorSubscription.dispose(); } });

      // Listen for file creation/deletion at the FS level
      const fsWatcher = fileSystemProvider.onDidChangeFile((events) => {
        for (const event of events) {
          const filename = uriToFilename(event.resource);
          if (!filename) continue;

          const isBaml = filename.endsWith('.baml');
          const isMedia = isMediaPath(filename);
          if (!isBaml && !isMedia) continue;

          // FileChangeType: 1=Updated, 2=Added, 3=Deleted
          if (event.type === FileChangeType.DELETED) {
            delete liveFiles[filename];
            if (isMedia) removeBlobUrl(filename);
            pushUpdate();
          } else if (event.type === FileChangeType.UPDATED || event.type === FileChangeType.ADDED) {
            const fileUri = vscode.Uri.file(`${rootPrefix}${filename}`);
            fileSystemProvider.readFile(fileUri).then((bytes: Uint8Array) => {
              if (disposed) return;
              if (isMedia) {
                liveFiles[filename] = toDataUrl(bytes, mimeFromPath(filename));
                updateBlobUrl(filename, bytes);
              } else {
                liveFiles[filename] = decoder.decode(bytes);
              }
              pushUpdate();
            }).catch(() => { /* file may not be readable yet */ });
          }
        }
      });
      disposables.push({ dispose: () => fsWatcher.dispose() });

      // ── Drag & drop handler ────────────────────────────────────────
      // The explorer's built-in upload handles drops and calls openEditor.
      // Our openEditor decorator (above) routes image files to the image
      // preview pane, so both drag-drop and the Upload button just work.

      // Handle given to backends so they can reflect server/worker-side
      // changes back into the editor without re-running workbench setup.
      const handle: WorkbenchHandle = {
        vscode,
        liveFiles,
        fileSystemProvider,
        encoder,
        decoder,
        updateBlobUrl,
        removeBlobUrl,
        notifyFilesChanged: (files) => onFilesChangeRef.current(files ?? { ...liveFiles }),
        isDisposed: () => disposed,
      };

      // ════════════════════════════════════════════════════════════════
      // Backend — connect LSP transport + runtime (re-run on reload)
      // ════════════════════════════════════════════════════════════════

      const connect = async () => {
        if (disposed) return;

        const { LanguageClientWrapper } = await import('monaco-languageclient/lcwrapper');
        if (disposed) return;

        const conn = await backendRef.current.connect(handle);
        if (disposed) { await conn.dispose(); return; }
        currentConnRef.current = conn;

        const lcWrapper = new LanguageClientWrapper({
          languageId: 'baml',
          clientOptions: {
            documentSelector: ['baml'],
          },
          connection: conn.lcConnection,
        });

        await lcWrapper.start();
        if (disposed) { await lcWrapper.dispose(); await conn.dispose(); return; }

        connDisposablesRef.current.push(
          { dispose: () => lcWrapper.dispose() },
          { dispose: () => conn.dispose() },
        );

        // Live-apply external on-disk edits the server pushes (remote backend
        // only — the worker backend's in-browser LSP never sends this), so the
        // editor stays in sync when files change underneath it (e.g. edited in
        // VS Code). The server already does echo-avoidance, so this won't fire
        // for the browser's own write-throughs.
        const lspClient = lcWrapper.getLanguageClient();
        if (lspClient) {
          const diskChangeSub = lspClient.onNotification(
            'baml/fileChangedOnDisk',
            async (params: { uri: string; text: string }) => {
              if (disposed) return;
              try {
                const fileUri = vscode.Uri.parse(params.uri);
                const target = fileUri.toString();
                const openDoc = vscode.workspace.textDocuments.find(
                  (d) => d.uri.toString() === target,
                );
                if (openDoc) {
                  if (openDoc.getText() === params.text) return; // already current
                  const lastLine = Math.max(openDoc.lineCount - 1, 0);
                  const end = openDoc.lineAt(lastLine).range.end;
                  const edit = new vscode.WorkspaceEdit();
                  edit.replace(
                    fileUri,
                    new vscode.Range(new vscode.Position(0, 0), end),
                    params.text,
                  );
                  await vscode.workspace.applyEdit(edit);
                } else {
                  // Not open: refresh the in-memory FS so the file tree and the
                  // next open show the current content.
                  await fileSystemProvider.writeFile(
                    fileUri,
                    encoder.encode(params.text),
                    { create: true, overwrite: true, unlock: false, atomic: false },
                  );
                }
                // The applyEdit above counts as a text change; clear it from the
                // unsaved indicator since this content came FROM disk.
                markFileSavedRef.current(target);
              } catch (err) {
                console.error('[MonacoEditor] applying external file change failed:', err);
              }
            },
          );
          connDisposablesRef.current.push({ dispose: () => diskChangeSub.dispose() });
        }

        const { setRuntimePort, setReloadCallback, setNavigateToSource } = await import('./ExecutionPanelPane');

        setRuntimePort(conn.runtimePort, { connectionVersion: connectionVersionRef.current });
        setReloadCallback(() => restartRef.current?.());
        setNavigateToSource((source) => {
          // Find a visible BAML editor to navigate to. promptfiddle typically
          // has a single .baml file open; this also covers multi-file projects
          // where the target file is currently visible.
          const bamlEditor = vscode.window.visibleTextEditors.find(
            (ed) => ed.document.uri.path.endsWith('.baml') || ed.document.languageId === 'baml'
          );
          const editor = bamlEditor ?? vscode.window.activeTextEditor;
          if (editor) {
            // line/column/endLine/endColumn are 0-indexed LSP positions, and
            // the backend expands end_* so the whole node span can be selected
            // directly (see SourceSpan in baml_compiler2_visualization).
            const start = new vscode.Position(source.line, source.column);
            const end =
              source.endLine != null && source.endColumn != null
                ? new vscode.Position(source.endLine, source.endColumn)
                : start;
            // anchor=start, active=end → the span is selected with the caret
            // at its end (kept inside the span for the graph round-trip check).
            editor.selection = new vscode.Selection(start, end);
            editor.revealRange(new vscode.Range(start, end), vscode.TextEditorRevealType.InCenter);
          }
        });

        connectionVersionRef.current += 1;
      };

      await connect();

      restartRef.current = () => {
        void (async () => {
          // Dispose the current connection (language client + transports), then reconnect.
          const toDispose = connDisposablesRef.current;
          connDisposablesRef.current = [];
          currentConnRef.current = null;
          for (const d of toDispose) {
            try {
              const r = d.dispose();
              if (r != null && typeof (r as { then?: unknown }).then === 'function') {
                await (r as Promise<unknown>);
              }
            } catch {
              /* no-op */
            }
          }
          // Yield before reconnecting so disposal side-effects settle.
          await new Promise((resolve) => setTimeout(resolve, 0));
          try {
            await connect();
            setConnectionCount((v) => v + 1);
          } catch (err: unknown) {
            console.error('[MonacoEditor] Reconnect failed:', err);
          }
        })();
      };
    })().catch((err: unknown) => {
      console.error('[MonacoEditor] Init failed:', err);
    });

    // ── Cleanup (unmount only) ──────────────────────────────────────
    return () => {
      restartRef.current = null;
      disposed = true;
      currentConnRef.current = null;
      for (const url of Object.values(blobUrlsRef.current)) {
        URL.revokeObjectURL(url);
      }
      blobUrlsRef.current = {};
      onBlobUrlsChangeRef.current?.({});
      const toDispose = connDisposablesRef.current;
      connDisposablesRef.current = [];
      for (const d of toDispose) {
        try { void d.dispose(); } catch { /* no-op */ }
      }
      for (const d of disposables) {
        try { d.dispose(); } catch { /* no-op */ }
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const isDev = isDevelopmentBuild();

  return (
    <div className="w-full relative overflow-hidden" style={{ height }}>
      {/* Skeleton shown until workbench is ready */}
      {!ready && (
        <div className="absolute inset-0 z-1">
          <EditorSkeleton height="100%" />
        </div>
      )}
      {/* Actual workbench mounts here */}
      <div ref={containerRef} className="nokey w-full h-full relative overflow-hidden" />
      {/* Dev-only: reload button (client-only to avoid hydration mismatch) */}
      {mounted && isDev && backend.supportsReload && (
        <button
          type="button"
          onClick={() => restartRef.current?.()}
          className="absolute bottom-2 right-2 z-10 flex items-center gap-2 rounded px-2 py-1 font-mono text-xs text-neutral-400 bg-black/50 border border-neutral-700 text-left cursor-pointer hover:bg-black/70 hover:border-neutral-600 transition-colors"
          title="Click to reconnect the BAML backend (loads fresh WASM)."
        >
          <span>Reconnect</span>
          <span className="text-sky-400/80" title="Connection count">#{connectionCount}</span>
        </button>
      )}
    </div>
  );
};

export default MonacoEditor;
