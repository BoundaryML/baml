// biome-ignore-all lint/style/useFilenamingConvention: Keep the existing public module path.
// biome-ignore-all lint/suspicious/noExplicitAny: Monaco's dynamic editor and filesystem internals are not publicly typed.
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

import { type FC, useEffect, useRef, useState } from 'react';
import './views-workbench.css';
import type { Dimension } from '@codingame/monaco-vscode-api/vscode/vs/base/browser/dom';
import type { IFileWriteOptions } from '@codingame/monaco-vscode-files-service-override';
import type {
  EditorBackend,
  EditorConnection,
  WorkbenchHandle,
} from './backend';
import { fromDataUrl, isMediaPath, mimeFromPath, toDataUrl } from './media';
import monospaceDarkTheme from './themes/monospace/monospace-dark.json';
import { createWorkspacePathModel } from './workspace-path';

declare const __DEV__: boolean | undefined;
declare const process: { env: { NODE_ENV?: string } } | undefined;

function isDevelopmentBuild(): boolean {
  if (typeof __DEV__ !== 'undefined') {
    return __DEV__;
  }
  return (
    typeof process !== 'undefined' && process.env.NODE_ENV === 'development'
  );
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
// Loading skeleton matches Monospace Dark so the transition is smooth.
// Token colors are inline because they're hardcoded to the pre-workbench theme;
// structural layout is package-owned CSS.
// ---------------------------------------------------------------------------

const sk = {
  accent: '#a87ffb',
  activityInactive: '#738295',
  bg: '#171f2b',
  border: '#333e4f',
  comment: '#7f8d9f',
  keyword: '#fd8da3',
  lineNum: '#475365',
  sidebar: '#10151d',
  status: '#1f2939',
  statusForeground: '#a4afbd',
  string: '#77d5a3',
  text: '#d9dfe7',
} as const;

const SkeletonLine: FC<{
  indent?: number;
  tokens: Array<{ w: number; color: string }>;
}> = ({ indent = 0, tokens }) => (
  <div
    className="baml-editor-skeleton-line"
    style={{ paddingLeft: indent * 16 }}
  >
    {tokens.map((t, i) => (
      <div
        className="baml-editor-skeleton-token"
        // biome-ignore lint/suspicious/noArrayIndexKey: Skeleton tokens are a static ordered list.
        key={i}
        style={{ background: t.color, width: t.w }}
      />
    ))}
  </div>
);

const EditorSkeleton: FC<{ height: string }> = ({ height }) => (
  <>
    <output className="baml-editor-loading-status">
      Loading the BAML editor.
    </output>
    <div
      aria-hidden="true"
      className="baml-editor-skeleton"
      style={{ background: sk.bg, height }}
    >
      <div
        className="baml-editor-skeleton-titlebar"
        style={{ background: sk.bg, borderColor: sk.border }}
      >
        <div
          className="baml-editor-skeleton-window-title"
          style={{ background: sk.text }}
        />
      </div>

      <div className="baml-editor-skeleton-main">
        <div
          className="baml-editor-skeleton-activitybar"
          style={{ background: sk.bg, borderColor: sk.border }}
        >
          {[0, 1, 2, 3].map((item) => (
            <div className="baml-editor-skeleton-activity-item" key={item}>
              {item === 0 && (
                <div
                  className="baml-editor-skeleton-activity-indicator"
                  style={{ background: sk.accent }}
                />
              )}
              <div
                className="baml-editor-skeleton-activity-icon"
                style={{
                  background: item === 0 ? sk.text : sk.activityInactive,
                }}
              />
            </div>
          ))}
        </div>

        <div
          className="baml-editor-skeleton-sidebar"
          style={{ background: sk.sidebar, borderColor: sk.border }}
        >
          <div className="baml-editor-skeleton-sidebar-title-row">
            <div
              className="baml-editor-skeleton-sidebar-title"
              style={{ background: sk.text }}
            />
          </div>
          {[90, 70, 110, 60].map((w, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: Skeleton rows are a static ordered list.
            <div className="baml-editor-skeleton-sidebar-row" key={i}>
              <div
                className="baml-editor-skeleton-sidebar-file"
                style={{ background: sk.text, width: w }}
              />
            </div>
          ))}
        </div>

        <div className="baml-editor-skeleton-editor">
          <div
            className="baml-editor-skeleton-gutter"
            style={{ background: sk.bg }}
          >
            {Array.from({ length: 12 }, (_, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: Skeleton lines are a static ordered list.
              <div className="baml-editor-skeleton-gutter-line" key={i}>
                <div
                  className="baml-editor-skeleton-line-number"
                  style={{ background: sk.lineNum }}
                />
              </div>
            ))}
          </div>

          <div className="baml-editor-skeleton-code">
            <SkeletonLine tokens={[{ color: sk.comment, w: 48 }]} />
            <SkeletonLine
              tokens={[
                { color: sk.keyword, w: 55 },
                { color: sk.text, w: 80 },
              ]}
            />
            <SkeletonLine
              indent={1}
              tokens={[
                { color: sk.keyword, w: 45 },
                { color: sk.text, w: 60 },
              ]}
            />
            <SkeletonLine
              indent={1}
              tokens={[
                { color: sk.keyword, w: 50 },
                { color: sk.string, w: 90 },
              ]}
            />
            <SkeletonLine tokens={[{ color: sk.text, w: 10 }]} />
            <SkeletonLine tokens={[]} />
            <SkeletonLine
              tokens={[
                { color: sk.keyword, w: 42 },
                { color: sk.text, w: 70 },
              ]}
            />
            <SkeletonLine
              indent={1}
              tokens={[
                { color: sk.keyword, w: 60 },
                { color: sk.text, w: 50 },
              ]}
            />
            <SkeletonLine
              indent={1}
              tokens={[
                { color: sk.string, w: 55 },
                { color: sk.string, w: 80 },
              ]}
            />
            <SkeletonLine
              indent={1}
              tokens={[
                { color: sk.keyword, w: 40 },
                { color: sk.string, w: 100 },
              ]}
            />
            <SkeletonLine tokens={[{ color: sk.text, w: 10 }]} />
            <SkeletonLine tokens={[]} />
          </div>
        </div>
      </div>

      <div
        className="baml-editor-skeleton-statusbar"
        style={{ background: sk.status, borderColor: sk.border }}
      >
        <div
          className="baml-editor-skeleton-status-item"
          style={{ background: sk.statusForeground, width: 44 }}
        />
        <div
          className="baml-editor-skeleton-status-item"
          style={{ background: sk.statusForeground, width: 92 }}
        />
      </div>
    </div>
  </>
);

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MonacoEditor: FC<MonacoEditorProps> = ({
  files,
  onFilesChange,
  backend,
  workspaceRoot = '/workspace',
  height = '100%',
  onBlobUrlsChange,
  autoSaveDelayMs,
  showSaveHint,
  onUnsavedChange,
}) => {
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
  const connDisposablesRef = useRef<
    Array<{ dispose: () => void | Promise<void> }>
  >([]);
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

  // biome-ignore lint/correctness/useExhaustiveDependencies: The workbench lifecycle is intentionally mount-only.
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
        {
          MonacoVscodeApiWrapper,
          defaultHtmlAugmentationInstructions,
          defaultViewsInit,
        },
        { createDefaultLocaleConfiguration },
        { useWorkerFactory: configureWorkerFactory, Worker: WorkerRef },
        keybindingsOverride,
        lifecycleOverride,
        localizationOverride,
        explorerOverride,
        filesOverride,
        bannerOverride,
        statusBarOverride,
        titleBarOverride,
        environmentOverride,
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
        import('@codingame/monaco-vscode-search-service-override'),
        import('@codingame/monaco-vscode-outline-service-override'),
        import('@codingame/monaco-vscode-secret-storage-service-override'),
        import('@codingame/monaco-vscode-storage-service-override'),
        import('vscode'),
        import('@b/pkg-grammar/baml.tmLanguage.json'),
      ]);

      if (disposed || !containerRef.current) return;

      // Convert the host-native root once. Everything inside Monaco uses the
      // resulting URI identity, never the raw path spelling.
      const workspacePaths = createWorkspacePathModel(
        vscode.Uri,
        workspaceRoot,
      );
      const workspaceFileUri = workspacePaths.configUri;

      const {
        InMemoryFileSystemProvider,
        registerFileSystemOverlay,
        FileChangeType,
      } = filesOverride;
      const rawFs = new InMemoryFileSystemProvider();
      const encoder = new TextEncoder();
      const decoder = new TextDecoder();
      const writeOpts: IFileWriteOptions = {
        atomic: false,
        create: true,
        overwrite: true,
        unlock: false,
      };

      /** Throws if the path is outside the sandbox. */
      const assertAllowed = (uri: import('vscode').Uri, op: string): void => {
        if (!workspacePaths.isAllowedUri(uri)) {
          throw new Error(
            `Sandbox violation: ${op} not allowed outside ${workspacePaths.rootPath} (got ${uri.path})`,
          );
        }
      };

      // Wrap the raw FS provider with sandbox checks via Proxy.
      const fileSystemProvider = new Proxy(rawFs, {
        get(target, prop, receiver) {
          const val = Reflect.get(target, prop, receiver);
          if (typeof val !== 'function') return val;

          switch (prop) {
            case 'writeFile':
              return (uri: any, content: any, opts: any) => {
                assertAllowed(uri, 'writeFile');
                return target.writeFile(uri, content, opts);
              };
            case 'mkdir':
              return (uri: any) => {
                assertAllowed(uri, 'mkdir');
                return target.mkdir(uri);
              };
            case 'delete':
              return (uri: any, opts: any) => {
                assertAllowed(uri, 'delete');
                workspacePaths.assertNotRootUri(uri, 'delete');
                return target.delete(uri, opts);
              };
            case 'rename':
              return (from: any, to: any, opts: any) => {
                assertAllowed(from, 'rename (source)');
                workspacePaths.assertNotRootUri(from, 'rename');
                assertAllowed(to, 'rename (target)');
                workspacePaths.assertNotRootUri(to, 'rename to');
                return target.rename(from, to, opts);
              };
            default:
              return val.bind(target);
          }
        },
      });

      // Create the workspace directory and all of its URI ancestors. Real
      // on-disk roots can have ancestors outside the sandbox, so use rawFs.
      for (const directoryUri of workspacePaths.rootAncestorUris()) {
        try {
          await rawFs.mkdir(directoryUri);
        } catch {
          /* already exists */
        }
      }

      // Write ALL persisted files to the in-memory FS.
      // Media files (images, etc.) are decoded from data URLs and also get blob URLs.
      const allFiles = Object.create(null) as Record<string, string>;
      for (const [filename, content] of Object.entries(filesRef.current)) {
        const normalizedFilename = workspacePaths.normalizeFilename(filename);
        if (normalizedFilename in allFiles) {
          throw new Error(
            `Duplicate workspace filename after normalization: ${filename}`,
          );
        }
        allFiles[normalizedFilename] = content;
      }
      const blobUrlMap: Record<string, string> = {};
      blobUrlsRef.current = blobUrlMap;

      for (const [filename, content] of Object.entries(allFiles)) {
        const fileUri = workspacePaths.fileUri(filename);
        for (const directoryUri of workspacePaths.parentDirectoryUris(
          filename,
        )) {
          try {
            await fileSystemProvider.mkdir(directoryUri);
          } catch {
            /* already exists */
          }
        }

        if (isMediaPath(filename)) {
          const bytes = fromDataUrl(content);
          await fileSystemProvider.writeFile(fileUri, bytes, writeOpts);
          const mime = mimeFromPath(filename);
          blobUrlMap[filename] = URL.createObjectURL(
            new Blob([new Uint8Array(bytes)], { type: mime }),
          );
        } else {
          await fileSystemProvider.writeFile(
            fileUri,
            encoder.encode(content),
            writeOpts,
          );
        }
      }
      onBlobUrlsChangeRef.current?.({ ...blobUrlMap });

      // Write workspace config
      await fileSystemProvider.writeFile(
        workspaceFileUri,
        encoder.encode(createWorkspaceContent(workspacePaths.rootPath)),
        writeOpts,
      );
      registerFileSystemOverlay(1, fileSystemProvider);

      const windowLabel = backendRef.current.windowLabel;

      // Init VS Code API wrapper and start the workbench
      const apiWrapper = new MonacoVscodeApiWrapper({
        $type: 'extended',
        extensions: [
          {
            config: {
              contributes: {
                commands: [
                  {
                    command: 'baml.openPlayground',
                    title: 'BAML: Open Playground',
                  },
                  {
                    command: 'baml.previewImage',
                    title: 'BAML: Preview Image',
                  },
                ],
                grammars: [
                  {
                    language: 'baml',
                    path: './baml.tmLanguage.json',
                    scopeName: 'source.baml',
                  },
                ],
                languages: [
                  {
                    aliases: ['BAML', 'baml'],
                    configuration: './language-configuration.json',
                    extensions: ['.baml'],
                    id: 'baml',
                  },
                ],
                themes: [
                  {
                    id: 'monospace-dark',
                    label: 'Monospace Dark',
                    path: './monospace-dark.json',
                    uiTheme: 'vs-dark',
                  },
                ],
              },
              engines: { vscode: '*' },
              name: 'baml-playground',
              publisher: 'boundaryml',
              version: '1.0.0',
            },
            filesOrContents: new Map<string, string | URL>([
              ['./baml.tmLanguage.json', JSON.stringify(bamlTmLanguageGrammar)],
              ['./monospace-dark.json', JSON.stringify(monospaceDarkTheme)],
              [
                './language-configuration.json',
                JSON.stringify({
                  autoClosingPairs: [
                    ['{', '}'],
                    ['[', ']'],
                    ['(', ')'],
                    { close: '"', open: '"' },
                    ['#"', '"#'],
                    ["'", "'"],
                    ['{#', '#}'],
                    ['{//', '//}'],
                  ],
                  brackets: [
                    ['{', '}'],
                    ['[', ']'],
                    ['(', ')'],
                  ],
                  comments: {
                    blockComment: ['{//', '//}'],
                    lineComment: '//',
                  },
                  surroundingPairs: [
                    ['{', '}'],
                    ['[', ']'],
                    ['(', ')'],
                    ['"', '"'],
                    ["'", "'"],
                  ],
                }),
              ],
            ]),
          },
        ],
        monacoWorkerFactory: () => {
          // Custom worker factory: the `new URL(..., import.meta.url)` patterns
          // must be in OUR source code (the consuming app transpiles pkg-editor)
          // so the bundler can resolve them at build time into proper asset URLs.
          // eslint-disable-next-line react-hooks/rules-of-hooks -- not a React hook
          configureWorkerFactory({
            workerLoaders: {
              editorWorkerService: () =>
                new WorkerRef(
                  new URL(
                    '@codingame/monaco-vscode-editor-api/esm/vs/editor/editor.worker.js',
                    import.meta.url,
                  ),
                  { type: 'module' },
                ),
              TextMateWorker: () =>
                new WorkerRef(
                  new URL(
                    '@codingame/monaco-vscode-textmate-service-override/worker',
                    import.meta.url,
                  ),
                  { type: 'module' },
                ),
            },
          });
        },
        serviceOverrides: {
          ...keybindingsOverride.default(),
          ...lifecycleOverride.default(),
          ...localizationOverride.default(createDefaultLocaleConfiguration()),
          ...bannerOverride.default(),
          ...statusBarOverride.default(),
          ...titleBarOverride.default(),
          ...explorerOverride.default(),
          ...environmentOverride.default(),
          ...secretStorageOverride.default(),
          ...storageOverride.default(),
          ...searchOverride.default(),
          ...outlineOverride.default(),
        },
        userConfiguration: {
          json: JSON.stringify({
            'editor.fontSize': 13,
            'editor.formatOnSave': true,
            'editor.lineHeight': 1.6,
            'editor.minimap.enabled': false,
            'editor.padding.top': 12,
            'editor.renderLineHighlight': 'line',
            'editor.scrollBeyondLastLine': false,
            'editor.semanticHighlighting.enabled': true,
            'editor.semanticTokenColorCustomizations': {
              rules: {
                'namespace:baml': '#808080CC',
              },
            },
            'editor.tabSize': 2,
            'editor.wordBasedSuggestions': 'off',
            'window.commandCenter': false,
            'window.titleSeparator': ' - ',
            'workbench.colorTheme': 'monospace-dark',
            'workbench.layoutControl.enabled': false,
            ...(autoSaveDelayMs != null
              ? {
                  'files.autoSave': 'afterDelay',
                  'files.autoSaveDelay': autoSaveDelayMs,
                }
              : {}),
          }),
        },
        viewsConfig: {
          $type: 'ViewsService',
          htmlAugmentationInstructions: defaultHtmlAugmentationInstructions,
          htmlContainer: containerRef.current,
          viewsInitFunc: defaultViewsInit,
        },
        workspaceConfig: {
          enableWorkspaceTrust: true,
          ...(windowLabel
            ? {
                windowIndicator: {
                  command: '',
                  label: windowLabel,
                  tooltip: '',
                },
              }
            : {}),
          configurationDefaults: {
            'window.title':
              // biome-ignore lint/suspicious/noTemplateCurlyInString: VS Code expands these placeholders.
              'BAML Playground${separator}${dirty}${activeEditorShort}',
          },
          productConfiguration: {
            nameLong: 'BAML Playground',
            nameShort: 'BAML Playground',
          },
          workspaceProvider: {
            async open() {
              return true;
            },
            trusted: true,
            workspace: { workspaceUri: workspaceFileUri },
          },
        },
      });

      await apiWrapper.start();
      if (disposed) return;

      // Register the ExecutionPanel as a custom editor pane in the workbench.
      // This must happen after start() so the workbench services are available.
      const { registerExecutionPanelPane } = await import(
        './ExecutionPanelPane'
      );
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
        const {
          SimpleEditorPane,
          SimpleEditorInput,
          registerEditorPane,
          EditorInputCapabilities,
        } = await import(
          '@codingame/monaco-vscode-api/service-override/tools/views'
        );
        const { StandaloneServices: SS } = await import(
          '@codingame/monaco-vscode-api'
        );
        const { IEditorService } = await import(
          '@codingame/monaco-vscode-api/vscode/vs/workbench/services/editor/common/editorService.service'
        );

        const IMAGE_PANE_ID = 'baml.imagePreview';
        const IMAGE_EXTS = new Set([
          'png',
          'jpg',
          'jpeg',
          'gif',
          'webp',
          'svg',
          'bmp',
          'ico',
        ]);
        class ImagePreviewInput extends SimpleEditorInput {
          constructor(uri: any) {
            super(uri);
            const name =
              String(uri.path ?? '')
                .split('/')
                .pop() ?? 'Image';
            this.setName(name);
            this.setTitle(name);
            this.addCapability(EditorInputCapabilities.Readonly);
          }
          get typeId() {
            return IMAGE_PANE_ID;
          }
          get editorId() {
            return IMAGE_PANE_ID;
          }
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

            const filename = workspacePaths.relativeFilename(uri);

            let dataUrl: string | undefined;
            if (filename) {
              dataUrl = liveFiles[filename];
            }

            if (!dataUrl) {
              try {
                const bytes: Uint8Array = await Promise.resolve(
                  rawFs.readFile(uri),
                );
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

            return {
              dispose() {
                img.remove();
              },
            };
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

        registerEditorPane(
          IMAGE_PANE_ID,
          'Image Preview',
          ImagePreviewPane as any,
          [ImagePreviewInput],
        );

        const editorService = SS.get(IEditorService);
        const origOpen = editorService.openEditor.bind(editorService);

        // @ts-expect-error override openEditor is expliclity desisred due to override
        editorService.openEditor = (
          input: any,
          optionsOrGroup?: any,
          group?: any,
        ) => {
          const resource = input?.resource ?? input?.original?.resource;
          const ext = resource?.path?.split('.')?.pop()?.toLowerCase() ?? '';
          if (resource && IMAGE_EXTS.has(ext)) {
            return origOpen(
              new ImagePreviewInput(resource),
              optionsOrGroup,
              group,
            );
          }
          return origOpen(input, optionsOrGroup, group);
        };

        vscode.commands.registerCommand('baml.previewImage', (uri?: any) => {
          const imageUri = uri ?? vscode.window.activeTextEditor?.document.uri;
          if (!imageUri) return;
          editorService.openEditor(new ImagePreviewInput(imageUri));
        });
      }

      // Register the code block renderer for hover markdown.
      // Without this, MarkdownRendererService._defaultCodeBlockRenderer is undefined
      // and all code fences in hover widgets render as empty <span> elements.
      {
        const { StandaloneServices } = await import(
          '@codingame/monaco-vscode-api'
        );
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

        const markdownService = StandaloneServices.get(
          IMarkdownRendererService,
        );
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

      // Close stale editors and collapse restored groups so the initial split is deterministic.
      await vscode.commands.executeCommand('workbench.action.closeAllGroups');
      if (disposed) return;

      // Determine which file to show — prefer main.baml, fall back to first text file
      const fileNames = Object.keys(allFiles).filter((f) => !isMediaPath(f));
      const firstFile =
        fileNames.find((path) => path.endsWith('main.baml')) ?? fileNames[0];

      if (firstFile) {
        const firstFileUri = workspacePaths.fileUri(firstFile);
        await vscode.workspace.openTextDocument(firstFileUri);
        if (disposed) return;
        await vscode.window.showTextDocument(firstFileUri);
        if (disposed) return;
      }

      await vscode.commands.executeCommand('baml.openPlayground');
      if (disposed) return;

      // Focus Explorer so file tree shows
      vscode.commands.executeCommand('workbench.view.explorer').then(
        () => {},
        () => {},
      );

      // Workbench ready — editor is visible, hide skeleton
      setReady(true);

      // ── Unsaved-changes indicator ────────────────────────────────────
      // For save-on-disk backends, drive a React-rendered badge (below) while
      // there are edits not yet written to disk. We track this from edit/save
      // events — monaco's own dirty-dot isn't surfaced in this workbench's
      // layout (no visible status bar). Externally-applied edits (pushed from
      // disk) are cleared via markFileSavedRef so they don't read as "unsaved".
      if (showSaveHint) {
        const refresh = () =>
          onUnsavedChangeRef.current?.(unsavedFilesRef.current.size > 0);
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
        blobUrlMap[filename] = URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: mime }),
        );
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
      const changeSubscription = vscode.workspace.onDidChangeTextDocument(
        (e) => {
          const filename = workspacePaths.relativeFilename(e.document.uri);
          if (filename?.endsWith('.baml')) {
            liveFiles[filename] = e.document.getText();
            pushUpdate();
          }
        },
      );
      disposables.push({ dispose: () => changeSubscription.dispose() });

      // Cursor position tracking for playground navigation
      let cursorDebounceTimer: ReturnType<typeof setTimeout> | undefined;
      const cursorSubscription = vscode.window.onDidChangeTextEditorSelection(
        (e: import('vscode').TextEditorSelectionChangeEvent) => {
          const filename = workspacePaths.relativeFilename(
            e.textEditor.document.uri,
          );
          if (!filename || !filename.endsWith('.baml')) return;

          clearTimeout(cursorDebounceTimer);
          cursorDebounceTimer = setTimeout(() => {
            const pos = e.selections[0]?.active;
            if (!pos) return;
            // VS Code API positions are 0-indexed, matching lsp_types::Position.
            currentConnRef.current?.onCursorMoved?.(
              filename,
              pos.line,
              pos.character,
            );
          }, 50);
        },
      );
      disposables.push({
        dispose: () => {
          clearTimeout(cursorDebounceTimer);
          cursorSubscription.dispose();
        },
      });

      // Listen for file creation/deletion at the FS level
      const fsWatcher = fileSystemProvider.onDidChangeFile((events) => {
        for (const event of events) {
          const filename = workspacePaths.relativeFilename(event.resource);
          if (!filename) continue;

          const isBaml = filename.endsWith('.baml');
          const isMedia = isMediaPath(filename);
          if (!isBaml && !isMedia) continue;

          // FileChangeType: 1=Updated, 2=Added, 3=Deleted
          if (event.type === FileChangeType.DELETED) {
            delete liveFiles[filename];
            if (isMedia) removeBlobUrl(filename);
            pushUpdate();
          } else if (
            event.type === FileChangeType.UPDATED ||
            event.type === FileChangeType.ADDED
          ) {
            const fileUri = workspacePaths.fileUri(filename);
            fileSystemProvider
              .readFile(fileUri)
              .then((bytes: Uint8Array) => {
                if (disposed) return;
                if (isMedia) {
                  liveFiles[filename] = toDataUrl(
                    bytes,
                    mimeFromPath(filename),
                  );
                  updateBlobUrl(filename, bytes);
                } else {
                  liveFiles[filename] = decoder.decode(bytes);
                }
                pushUpdate();
              })
              .catch(() => {
                /* file may not be readable yet */
              });
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
        decoder,
        encoder,
        fileSystemProvider,
        isDisposed: () => disposed,
        liveFiles,
        notifyFilesChanged: (files) =>
          onFilesChangeRef.current(files ?? { ...liveFiles }),
        removeBlobUrl,
        updateBlobUrl,
        vscode,
      };

      // ════════════════════════════════════════════════════════════════
      // Backend — connect LSP transport + runtime (re-run on reload)
      // ════════════════════════════════════════════════════════════════

      const connect = async () => {
        if (disposed) return;

        const { LanguageClientWrapper } = await import(
          'monaco-languageclient/lcwrapper'
        );
        if (disposed) return;

        const conn = await backendRef.current.connect(handle);
        if (disposed) {
          await conn.dispose();
          return;
        }
        currentConnRef.current = conn;

        const lcWrapper = new LanguageClientWrapper({
          clientOptions: {
            documentSelector: ['baml'],
          },
          connection: conn.lcConnection,
          languageId: 'baml',
        });

        await lcWrapper.start();
        if (disposed) {
          await lcWrapper.dispose();
          await conn.dispose();
          return;
        }

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
                    {
                      atomic: false,
                      create: true,
                      overwrite: true,
                      unlock: false,
                    },
                  );
                }
                // The applyEdit above counts as a text change; clear it from the
                // unsaved indicator since this content came FROM disk.
                markFileSavedRef.current(target);
              } catch (err) {
                console.error(
                  '[MonacoEditor] applying external file change failed:',
                  err,
                );
              }
            },
          );
          connDisposablesRef.current.push({
            dispose: () => diskChangeSub.dispose(),
          });
        }

        const { setRuntimePort, setReloadCallback, setNavigateToSource } =
          await import('./ExecutionPanelPane');

        setRuntimePort(conn.runtimePort, {
          connectionVersion: connectionVersionRef.current,
        });
        setReloadCallback(() => restartRef.current?.());
        setNavigateToSource(async (source) => {
          if (!source.filePath) {
            return;
          }

          // `source.filePath` comes from the language server's
          // `controlFlowGraphResult.graph.nodes[nodeId].sourceSpan`. Do not infer the
          // target from the active editor: an inlined graph node may live in another
          // BAML file.
          const targetUri = vscode.Uri.file(source.filePath);
          const visibleEditor = vscode.window.visibleTextEditors.find(
            (editor) => editor.document.uri.toString() === targetUri.toString(),
          );
          const sourceViewColumn =
            visibleEditor?.viewColumn ??
            vscode.window.visibleTextEditors.find(
              (editor) =>
                editor.document.uri.path.endsWith('.baml') ||
                editor.document.languageId === 'baml',
            )?.viewColumn ??
            vscode.ViewColumn.One;
          const document =
            visibleEditor?.document ??
            (await vscode.workspace.openTextDocument(targetUri));
          const editor = await vscode.window.showTextDocument(document, {
            preserveFocus: false,
            preview: false,
            viewColumn: sourceViewColumn,
          });

          // The language server also sends line/column/endLine/endColumn unchanged in
          // `sourceSpan`. All four are zero-indexed LSP positions, and the end is
          // exclusive. For example, given this source as displayed to a human:
          //
          //   1 | function demo() {
          //   2 |   call()
          //   3 | }
          //
          // the `call()` node arrives as:
          // `{ line: 1, column: 2, endLine: 1, endColumn: 8 }`.
          // VS Code's Position uses that same zero-indexed convention, so no +/- 1
          // conversion belongs here.
          const start = new vscode.Position(source.line, source.column);
          const end =
            source.endLine != null && source.endColumn != null
              ? new vscode.Position(source.endLine, source.endColumn)
              : start;
          const range = new vscode.Range(start, end);
          editor.selection = new vscode.Selection(start, end);
          editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
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
              if (
                r != null &&
                typeof (r as { then?: unknown }).then === 'function'
              ) {
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
        try {
          void d.dispose();
        } catch {
          /* no-op */
        }
      }
      for (const d of disposables) {
        try {
          d.dispose();
        } catch {
          /* no-op */
        }
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
      <div
        className="nokey w-full h-full relative overflow-hidden"
        ref={containerRef}
      />
      {/* Dev-only: reload button (client-only to avoid hydration mismatch) */}
      {mounted && isDev && backend.supportsReload && (
        <button
          className="absolute bottom-2 right-2 z-10 flex items-center gap-2 rounded px-2 py-1 font-mono text-xs text-neutral-400 bg-black/50 border border-neutral-700 text-left cursor-pointer hover:bg-black/70 hover:border-neutral-600 transition-colors"
          onClick={() => restartRef.current?.()}
          title="Click to reconnect the BAML backend (loads fresh WASM)."
          type="button"
        >
          <span>Reconnect</span>
          <span className="text-sky-400/80" title="Connection count">
            #{connectionCount}
          </span>
        </button>
      )}
    </div>
  );
};

export default MonacoEditor;
