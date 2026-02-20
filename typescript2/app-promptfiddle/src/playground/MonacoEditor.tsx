/**
 * MonacoEditor — BAML editor with file tree (explorer) and LSP.
 *
 * Initialization is split into two phases so the editor is visible immediately:
 *
 *   Workbench (fast): Init VS Code API wrapper + create editor with text.
 *                     The user sees the editor with their code right away.
 *
 *   Worker + LSP (async): Spawn WASM worker, start language client, open Explorer.
 *                         LSP features (hover, diagnostics, completions) light up
 *                         once the worker is ready.
 */

import { useEffect, useRef, useState, type FC } from 'react';
import { useSetAtom } from 'jotai';
import './views-workbench.css';
import { type IFileWriteOptions } from '@codingame/monaco-vscode-files-service-override';
import { blobUrlsAtom } from './PlaygroundProvider';
import type { Dimension } from '@codingame/monaco-vscode-api/vscode/vs/base/browser/dom';

// ---------------------------------------------------------------------------
// Media file helpers
// ---------------------------------------------------------------------------

const MIME_TYPES: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif',
  svg: 'image/svg+xml', webp: 'image/webp', ico: 'image/x-icon', bmp: 'image/bmp',
  mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg',
  mp4: 'video/mp4', webm: 'video/webm',
  pdf: 'application/pdf',
};

function mimeFromPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return MIME_TYPES[ext] ?? 'application/octet-stream';
}

function isMediaPath(filename: string): boolean {
  const ext = filename.split('.').pop()?.toLowerCase() ?? '';
  return ext in MIME_TYPES;
}

/** Encode binary data as a data URL. */
function toDataUrl(data: Uint8Array, mime: string): string {
  let binary = '';
  for (const byte of data) binary += String.fromCharCode(byte);
  return `data:${mime};base64,${btoa(binary)}`;
}

/** Decode a data URL back to binary. */
function fromDataUrl(dataUrl: string): Uint8Array {
  const base64 = dataUrl.split(',')[1] ?? '';
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
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
  /** CSS height for the container. Defaults to '100%'. */
  height?: string;
  /** Called when the worker is ready — exposes the Worker so SplitPreview can send RPC messages. */
  onWorkerReady?: (worker: Worker) => void;
}

function createWorkspaceContent(workspacePath: string): string {
  return JSON.stringify({ folders: [{ path: workspacePath }] }, null, 2);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

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

export const MonacoEditor: FC<MonacoEditorProps> = ({ files, onFilesChange, height = '100%', onWorkerReady }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const onFilesChangeRef = useRef(onFilesChange);
  const onWorkerReadyRef = useRef(onWorkerReady);
  const filesRef = useRef(files);
  const [ready, setReady] = useState(false);
  const [workerVersion, setWorkerVersion] = useState(0);
  const [wasmBuildTime, setWasmBuildTime] = useState<number | null>(null);
  const [mounted, setMounted] = useState(false);
  const workerRef = useRef<Worker | null>(null);
  const workerLspDisposablesRef = useRef<Array<{ dispose: () => void }>>([]);
  const workbenchContextRef = useRef<Record<string, unknown> | null>(null);
  /** Increments each time we connect worker+LSP; used as React key to force ExecutionPanel remount. */
  const connectionVersionRef = useRef(0);
  /** Callback to restart the worker; set once connectWorkerAndLsp is defined. */
  const restartWorkerRef = useRef<(() => void) | null>(null);
  const setBlobUrls = useSetAtom(blobUrlsAtom);

  useEffect(() => {
    setMounted(true);
  }, []);

  onFilesChangeRef.current = onFilesChange;
  onWorkerReadyRef.current = onWorkerReady;
  filesRef.current = files;

  useEffect(() => {
    if (!containerRef.current) return;

    let disposed = false;
    const disposables: Array<{ dispose: () => void }> = [];
    let worker: Worker | null = null;

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
        import('./baml.tmLanguage.json'),
      ]);

      if (disposed || !containerRef.current) return;

      // Set up in-memory filesystem
      const workspaceFolderUri = vscode.Uri.file('/workspace');
      const workspaceFileUri = vscode.Uri.file('/workspace.code-workspace');

      const { InMemoryFileSystemProvider, registerFileSystemOverlay, FileChangeType } = filesOverride;
      const rawFs = new InMemoryFileSystemProvider();
      const encoder = new TextEncoder();
      const decoder = new TextDecoder();
      const writeOpts: IFileWriteOptions = { atomic: false, unlock: false, create: true, overwrite: true };

      // Sandbox: only allow operations inside /workspace (plus the workspace config file).
      const WORKSPACE_ROOT = '/workspace';
      const WORKSPACE_CONFIG = '/workspace.code-workspace';

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

      // Create workspace directory
      await fileSystemProvider.mkdir(workspaceFolderUri);

      // Write ALL persisted files to the in-memory FS.
      // Media files (images, etc.) are decoded from data URLs and also get blob URLs.
      const allFiles = filesRef.current;
      const blobUrlMap: Record<string, string> = {};

      for (const [filename, content] of Object.entries(allFiles)) {
        const absPath = filename.startsWith('/workspace/') ? filename : `/workspace/${filename}`;
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
      setBlobUrls({ ...blobUrlMap });

      // Write workspace config
      await fileSystemProvider.writeFile(
        workspaceFileUri,
        encoder.encode(createWorkspaceContent('/workspace')),
        writeOpts,
      );
      registerFileSystemOverlay(1, fileSystemProvider);

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
          windowIndicator: { label: 'BAML Playground — Edit, compile, and run BAML in the browser', tooltip: '', command: '' },
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
          // must be in OUR source code (not node_modules) so the bundler can
          // resolve them at build time into proper asset URLs.
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
        const WORKSPACE_PREFIX = '/workspace/';

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
      const firstFileUri = vscode.Uri.file(`/workspace/${firstFile}`);

      // Open the document and show it in the editor
      await vscode.workspace.openTextDocument(firstFileUri);
      if (disposed) return;
      await vscode.window.showTextDocument(firstFileUri);
      if (disposed) return;

      // Focus Explorer so file tree shows
      vscode.commands.executeCommand('workbench.view.explorer').then(() => {}, () => {});

      // Workbench ready — editor is visible, hide skeleton
      setReady(true);

      // ── Track live file state ────────────────────────────────────────
      // Single mutable map for all files (text + media).
      // Text files store raw content, media files store data URLs.
      const liveFiles: Record<string, string> = { ...allFiles };

      /** Helper: extract just the filename from a vscode Uri under /workspace/ */
      const uriToFilename = (uri: { path: string }): string | null => {
        const prefix = '/workspace/';
        if (uri.path.startsWith(prefix)) {
          return uri.path.slice(prefix.length);
        }
        return null;
      };

      /** Notify parent of the latest file state and push to worker VFS. */
      const pushUpdate = () => {
        onFilesChangeRef.current({ ...liveFiles });
        if (workerRef.current) {
          workerRef.current.postMessage({ type: 'filesChanged', files: { ...liveFiles } });
        }
      };

      /** Create a blob URL for a media file and update the atom. */
      const updateBlobUrl = (filename: string, bytes: Uint8Array) => {
        if (blobUrlMap[filename]) {
          URL.revokeObjectURL(blobUrlMap[filename]);
        }
        const mime = mimeFromPath(filename);
        blobUrlMap[filename] = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mime }));
        setBlobUrls({ ...blobUrlMap });
      };

      /** Remove a blob URL for a deleted media file. */
      const removeBlobUrl = (filename: string) => {
        if (blobUrlMap[filename]) {
          URL.revokeObjectURL(blobUrlMap[filename]);
          delete blobUrlMap[filename];
          setBlobUrls({ ...blobUrlMap });
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
            const fileUri = vscode.Uri.file(`/workspace/${filename}`);
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

      // Store workbench context so worker+LSP (and restart) can use it without re-running workbench setup.
      workbenchContextRef.current = {
        liveFiles,
        fileSystemProvider,
        encoder,
        decoder,
        blobUrlMap,
        updateBlobUrl,
        removeBlobUrl,
        vscode,
        allFiles,
      };

      // ════════════════════════════════════════════════════════════════
      // Worker + LSP — connect WASM worker and language client (re-run on restart)
      // ════════════════════════════════════════════════════════════════

      const connectWorkerAndLsp = async () => {
        const ctx = workbenchContextRef.current;
        if (!ctx || disposed) return;
        const {
          liveFiles: lf,
          fileSystemProvider: fsp,
          encoder: enc,
          decoder: dec,
          blobUrlMap: blobs,
          updateBlobUrl: updBlob,
          removeBlobUrl: remBlob,
          vscode: vs,
          allFiles: af,
        } = ctx as {
          liveFiles: Record<string, string>;
          fileSystemProvider: { mkdir: (u: unknown) => Promise<void>; writeFile: (u: unknown, c: unknown, o: unknown) => Promise<void>; delete: (u: unknown, o: unknown) => Promise<void> };
          encoder: TextEncoder;
          decoder: TextDecoder;
          blobUrlMap: Record<string, string>;
          updateBlobUrl: (filename: string, bytes: Uint8Array) => void;
          removeBlobUrl: (filename: string) => void;
          vscode: typeof import("vscode");
          allFiles: Record<string, string>;
        };
        const workerLspDisposables = workerLspDisposablesRef.current;

        const { LanguageClientWrapper } = await import('monaco-languageclient/lcwrapper');
        const { BrowserMessageReader, BrowserMessageWriter } = await import('vscode-languageclient/browser');

        if (disposed) return;

        // Spawn worker — WASM loads inside the worker, doesn't block main thread
        worker = new Worker(
          new URL('./baml-lsp-worker.ts', import.meta.url),
          { type: 'module', name: 'BAML Worker' },
        );
        workerRef.current = worker;

      // Listen for the 'ready' message IMMEDIATELY — before any awaits —
      // so we don't miss it if WASM loads fast (e.g. from cache).
      const workerReadyPromise = new Promise<void>((resolve) => {
        const onMsg = (event: MessageEvent) => {
          if (event.data?.type === 'ready') {
            worker!.removeEventListener('message', onMsg);
            resolve();
          }
        };
        worker!.addEventListener('message', onMsg);
      });

        // Listen for VFS mutations from the WASM runtime (worker → main).
        const onVfsChange = (event: MessageEvent) => {
          if (disposed) return;
          const data = event.data;
          if (data?.type === 'vfsFileChanged') {
            const { path: relPath, content } = data as { path: string; content: string };
            lf[relPath] = content;
            const absPath = `/workspace/${relPath}`;
            const isMedia = isMediaPath(relPath);
            const bytes = isMedia ? fromDataUrl(content) : enc.encode(content);
            (async () => {
              const parts = absPath.split('/');
              for (let i = 2; i < parts.length; i++) {
                const parentPath = parts.slice(0, i).join('/');
                try { await fsp.mkdir(vs.Uri.file(parentPath)); } catch { /* exists */ }
              }
              await fsp.writeFile(vs.Uri.file(absPath), bytes, { create: true, overwrite: true, unlock: false, atomic: false });
              if (isMedia) updBlob(relPath, bytes);
            })();
            onFilesChangeRef.current({ ...lf });
          } else if (data?.type === 'vfsFileDeleted') {
            const { path: relPath } = data as { path: string };
            delete lf[relPath];
            const absPath = `/workspace/${relPath}`;
            fsp.delete(vs.Uri.file(absPath), { recursive: false, useTrash: false, atomic: false }).catch(() => {});
            if (isMediaPath(relPath)) remBlob(relPath);
            onFilesChangeRef.current({ ...lf });
          } else if (data?.type === 'buildTime') {
            const { value } = data as { value: string };
            setWasmBuildTime(Number(value) || null);
          }
        };
        worker!.addEventListener('message', onVfsChange);
        workerLspDisposables.push({ dispose: () => worker?.removeEventListener('message', onVfsChange) });

        const channel = new MessageChannel();
        worker.postMessage(
          {
            port: channel.port2,
            initialFiles: { ...lf },
            rootPath: '/workspace',
          },
          [channel.port2],
        );

        const reader = new BrowserMessageReader(channel.port1);
        const writer = new BrowserMessageWriter(channel.port1);

        if (disposed) { worker.terminate(); return; }

        const lcWrapper = new LanguageClientWrapper({
          languageId: 'baml',
          clientOptions: {
            documentSelector: ['baml'],
          },
          connection: {
            options: { $type: 'WorkerDirect', worker, messagePort: channel.port1 },
            messageTransports: { reader, writer },
          },
        });

        await lcWrapper.start();
        if (disposed) { worker.terminate(); return; }
        workerLspDisposables.push({ dispose: () => lcWrapper.dispose() });

        await workerReadyPromise;
        if (disposed) { worker.terminate(); return; }

        const { setRuntimePort } = await import('./ExecutionPanelPane');
        const { WorkerRuntimePort } = await import('@b/pkg-playground');

        const runtimePort = new WorkerRuntimePort(worker!);
        workerLspDisposables.push(runtimePort);
        onWorkerReadyRef.current?.(worker!);
        setRuntimePort(runtimePort, { connectionVersion: connectionVersionRef.current });

        connectionVersionRef.current += 1;
      };

      await connectWorkerAndLsp();

      restartWorkerRef.current = () => {
        void (async () => {
          const w = workerRef.current;
          // Dispose LSP and worker, then reconnect.
          const toDispose = workerLspDisposablesRef.current;
          workerLspDisposablesRef.current = [];
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
          if (w) {
            try {
              w.postMessage({ type: "dispose" });
            } catch {
              /* worker may already be terminated */
            }
            w.terminate();
          }
          workerRef.current = null;
          setWasmBuildTime(null);
          // Yield before reconnecting so disposal side-effects settle.
          await new Promise((resolve) => setTimeout(resolve, 0));
          try {
            await connectWorkerAndLsp();
            setWorkerVersion((v) => v + 1);
          } catch (err: unknown) {
            console.error('[MonacoEditor] Restart failed:', err);
          }
        })();
      };
    })().catch((err: unknown) => {
      console.error('[MonacoEditor] Init failed:', err);
    });

    // ── Cleanup (unmount only; restart only disposes worker+LSP) ──────
    return () => {
      restartWorkerRef.current = null;
      disposed = true;
      workbenchContextRef.current = null;
      workerRef.current = null;
      setWasmBuildTime(null);
      for (const d of workerLspDisposablesRef.current) {
        try { d.dispose(); } catch { /* no-op */ }
      }
      workerLspDisposablesRef.current = [];
      for (const d of disposables) {
        try { d.dispose(); } catch { /* no-op */ }
      }
      if (worker) {
        worker.terminate();
        worker = null;
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const isDev = process.env.NODE_ENV === "development";

  return (
    <div className="w-full relative overflow-hidden" style={{ height }}>
      {/* Skeleton shown until workbench is ready */}
      {!ready && (
        <div className="absolute inset-0 z-1">
          <EditorSkeleton height="100%" />
        </div>
      )}
      {/* Actual workbench mounts here */}
      <div ref={containerRef} className="w-full h-full relative overflow-hidden" />
      {/* Dev-only: restart button + worker info (client-only to avoid hydration mismatch) */}
      {mounted && isDev && (
        <button
          type="button"
          onClick={() => restartWorkerRef.current?.()}
          className="absolute bottom-2 right-2 z-10 flex flex-col gap-0.5 rounded px-2 py-1 font-mono text-xs text-neutral-400 bg-black/50 border border-neutral-700 text-left cursor-pointer hover:bg-black/70 hover:border-neutral-600 transition-colors"
          title="Click to restart the BAML worker (loads fresh WASM)."
        >
          <div className="flex items-center gap-2">
            <span>Worker v{workerVersion}</span>
            <span className="text-sky-400/80" title="Connection version (port identity)">
              port:{connectionVersionRef.current}
            </span>
          </div>
          {wasmBuildTime != null && (() => {
            const d = new Date(wasmBuildTime * 1000);
            const abs = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
            const delta = Math.floor(Date.now() / 1000) - wasmBuildTime;
            const rel = delta < 60 ? `${delta}s ago`
              : delta < 3600 ? `${Math.floor(delta / 60)}m ago`
              : delta < 86400 ? `${Math.floor(delta / 3600)}h ago`
              : `${Math.floor(delta / 86400)}d ago`;
            return (
              <span className="truncate max-w-[250px]">
                Built: {abs} ({rel})
              </span>
            );
          })()}
        </button>
      )}
    </div>
  );
};

export default MonacoEditor;
