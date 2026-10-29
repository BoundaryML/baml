'use client';

import {
  configureProxyEnvVar,
  ExecutionPanel,
  initPlaygroundEnv,
  type RuntimePort,
  type SourceNavigationTarget,
  WorkerRuntimePort,
} from '@b/pkg-playground';
import Editor, { type BeforeMount, type OnMount } from '@monaco-editor/react';
import { useCallback, useRef, useState } from 'react';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import {
  createInitializedBamlWorker,
  getBamlWorker,
} from '@/playground/spawnBamlWorker';
import { useCodeTheme } from '../_lib/code-theme';
import { registerBaml } from '../_lib/baml-monarch';
// Self-contained styling: embeds outside the /learn decks (homepage hero)
// don't import learn2.css at the page level, so bring the l2-live styles in.
import '../learn2.css';

// Route the WASM runtime's LLM requests through the Boundary gateway proxy (a
// Cloudflare Worker that injects our provider API keys server-side and bypasses
// CORS) so visitors can run the playground with no keys of their own. The
// runtime reads BOUNDARY_PROXY_URL and prepends it to each request URL, so it
// must include a scheme. `visible: true` surfaces the gateway on/off toggle in
// the env-vars dialog. Configured at import, before the ExecutionPanel mounts.
configureProxyEnvVar({ url: 'https://proxy.promptfiddle.com', visible: true });

interface LivePlaygroundProps {
  initialCode: string;
  /** Optional filename strip above the editor pane (e.g. "pipeline.baml"). */
  filename?: string;
  /** Function to preselect in the panel (e.g. the pipeline entrypoint). */
  initialFunction?: string;
  /** Panel tab to open on (default 'graph' — the viz is the point). */
  initialTab?: 'run' | 'graph' | 'prompt' | 'curl';
  /** Per-function example args, seeded into the args editor on selection. */
  argsByFunction?: Record<string, string>;
  /** Fill the parent instead of the deck-slide height cap (hero embeds). */
  fill?: boolean;
  /** Whether the panel's function/tests sidebar starts open (default true). */
  initialSidebarOpen?: boolean;
  /** Lines to tint (whole-line) on the initial code — e.g. the entry function. */
  highlightLines?: number[];
  /** Fires once the runtime has produced the first graph for this code — used
   *  by callers (e.g. a tabbed switcher) to clear a "loading" indicator. */
  onReady?: () => void;
  /** Text for the loading veil shown over the result/graph pane (not the
   *  editor) until the first graph for this code is ready. Defaults to
   *  "Loading…". Pass to label which workflow is loading. */
  loadingLabel?: string;
  /** Use a dedicated worker instead of the page-shared one. Required when a
   *  page mounts more than one playground at once — the shared worker keys
   *  every project on the same `baml_src/main.baml`, so siblings would clobber
   *  each other's project state. The worker is terminated on unmount. */
  isolated?: boolean;
}

type EditorInstance = Parameters<OnMount>[0];
type MonacoNs = Parameters<OnMount>[1];
type DecorationsCollection = ReturnType<
  EditorInstance['createDecorationsCollection']
>;

// LSP `textDocument/publishDiagnostics` payload shape.
interface LspPos {
  line: number;
  character: number;
}
interface LspDiagnostic {
  range: { start: LspPos; end: LspPos };
  severity?: number; // 1=Error 2=Warning 3=Info 4=Hint
  message: string;
  source?: string;
}
interface LspPublish {
  uri?: string;
  diagnostics?: LspDiagnostic[];
}

function severityClass(sev?: number): 'error' | 'warning' | 'info' {
  if (sev === 2) return 'warning';
  if (sev === 3 || sev === 4) return 'info';
  return 'error';
}

/**
 * Live BAML playground: Monaco editor (left) + the real BexVM ExecutionPanel
 * (right), sharing one worker. Setup happens in Monaco's `beforeMount`/`onMount`
 * callbacks and event handlers — no `useEffect`.
 *
 * Diagnostics: the worker forwards LSP `publishDiagnostics`; we render them as
 * Monaco markers (squiggles + hover) plus inline ErrorLens-style messages.
 */
export default function LivePlayground({
  initialCode,
  filename,
  initialFunction,
  initialTab = 'graph',
  argsByFunction,
  fill,
  initialSidebarOpen,
  highlightLines,
  onReady,
  loadingLabel = 'Loading…',
  isolated = false,
}: LivePlaygroundProps) {
  const [port, setPort] = useState<RuntimePort | null>(null);
  const [version, setVersion] = useState(0);
  const [failed, setFailed] = useState(false);
  // The result/graph pane is veiled until the first graph for this code is
  // ready (only the pane — never the editor).
  const [ready, setReady] = useState(false);
  // Monaco theme chosen by the page (CodeThemeProvider); ref so the stable
  // onMount callback reads it.
  const codeTheme = useCodeTheme();
  const monacoThemeRef = useRef(codeTheme.monaco);
  monacoThemeRef.current = codeTheme.monaco;
  const portRef = useRef<RuntimePort | null>(null);
  const codeRef = useRef(initialCode);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cursorDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoNs | null>(null);
  // `onReady` fires once, on the first graph result for this mount. Keep it in
  // a ref so the stable onMount callback always sees the latest prop.
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  const readyFiredRef = useRef(false);
  // Worker + the exact listener this mount attached, so we can detach on
  // unmount (the worker is shared, so leaking listeners across tab switches
  // piles up zombie handlers and slows every later switch).
  const workerRef = useRef<Worker | null>(null);
  const msgHandlerRef = useRef<((e: MessageEvent) => void) | null>(null);
  // True when this mount owns a dedicated (isolated) worker — terminate it on
  // unmount. The shared worker is never terminated (other mounts reuse it).
  const ownsWorkerRef = useRef(false);
  // Don't treat the worker's pre-existing (previous example's) project state as
  // "ready" — only fire onReady once we've pushed THIS mount's code.
  const nudgedRef = useRef(false);
  const lensRef = useRef<DecorationsCollection | null>(null);

  const applyDiagnostics = useCallback((publish: LspPublish) => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    if (!editor || !monaco || !model) return;

    const diags = publish.diagnostics ?? [];

    // Squiggles + hover via markers.
    monaco.editor.setModelMarkers(
      model,
      'baml',
      diags.map((d) => {
        const sev =
          d.severity === 2
            ? monaco.MarkerSeverity.Warning
            : d.severity === 3 || d.severity === 4
              ? monaco.MarkerSeverity.Info
              : monaco.MarkerSeverity.Error;
        return {
          endColumn: d.range.end.character + 1,
          endLineNumber: d.range.end.line + 1,
          message: d.message,
          severity: sev,
          startColumn: d.range.start.character + 1,
          startLineNumber: d.range.start.line + 1,
        };
      }),
    );

    // Inline ErrorLens-style message at the end of each errored line.
    try {
      if (!lensRef.current) {
        lensRef.current = editor.createDecorationsCollection();
      }
      const decos = diags.map((d) => {
        const line = d.range.start.line + 1;
        const kind = severityClass(d.severity);
        const endCol = model.getLineMaxColumn(line);
        // The inline (decoration) message is truncated; the squiggle hover
        // still carries the full message.
        const inline =
          d.message.length > 100 ? `${d.message.slice(0, 100)}…` : d.message;
        return {
          options: {
            // Whole-line tint marking the errored span, plus the inline
            // ErrorLens message at the end of the line.
            className: `l2-el-line-${kind}`,
            isWholeLine: true,
            after: {
              content: `    ${inline}`,
              inlineClassName: `l2-el-msg l2-el-msg-${kind}`,
              inlineClassNameAffectsLetterSpacing: true,
            },
          },
          range: new monaco.Range(line, 1, line, endCol),
        };
      });
      lensRef.current.set(decos);
    } catch (e) {
      // eslint-disable-next-line no-console
      console.error('[learn2 lens] failed to set inline decorations', e);
    }
  }, []);

  // Cursor → playground selection. The runtime resolves the function/test under
  // the caret and emits a `cursorContext` notification, which the ExecutionPanel
  // already consumes to focus that function. Monaco positions are 1-indexed; the
  // runtime wants 0-indexed (lsp_types::Position). `find_source_file` suffix-
  // matches, so the relative `baml_src/main.baml` resolves to the project file.
  const postCursor = useCallback(() => {
    const pos = editorRef.current?.getPosition();
    if (!pos) return;
    portRef.current?.postMessage({
      column: pos.column - 1,
      file: 'baml_src/main.baml',
      line: pos.lineNumber - 1,
      type: 'cursorPosition',
    });
  }, []);

  // Graph node click → select that node's source span in the editor. Spans
  // are 0-indexed LSP positions; Monaco is 1-indexed. No focus() — selection
  // renders unfocused, and stealing focus would swallow deck arrow keys.
  const onNavigateToSource = useCallback((src: SourceNavigationTarget) => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;
    const range = new monaco.Range(
      src.line + 1,
      src.column + 1,
      (src.endLine ?? src.line) + 1,
      (src.endColumn ?? src.column) + 1,
    );
    editor.setSelection(range);
    editor.revealRangeInCenterIfOutsideViewport(range);
  }, []);

  const beforeMount: BeforeMount = useCallback((monaco) => {
    registerBaml(monaco);
  }, []);

  const onMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      monaco.editor.setTheme(monacoThemeRef.current);

      // Seed playground env defaults once (placeholder provider keys + gateway
      // on) into pkg-playground's defaultSessionStore — the same store the
      // ExecutionPanel's `useEnvVars(port)` pushes to the worker. Idempotent
      // (localStorage-gated) and user-entered keys always win, so a per-mount
      // call never clobbers anything. No useEffect — done at editor mount.
      initPlaygroundEnv();

      // Showcase highlight: a static whole-line tint marking the entry
      // function of the shipped snippet (not tracked across edits).
      if (highlightLines?.length) {
        editor.createDecorationsCollection(
          highlightLines.map((line) => ({
            options: { className: 'l6-wf-hl', isWholeLine: true },
            range: new monaco.Range(line, 1, line, 1),
          })),
        );
      }

      // Forward caret moves (debounced) so clicking in the code selects the
      // matching function/test in the playground panel.
      editor.onDidChangeCursorPosition(() => {
        if (cursorDebounceRef.current) clearTimeout(cursorDebounceRef.current);
        cursorDebounceRef.current = setTimeout(postCursor, 50);
      });

      ownsWorkerRef.current = isolated;
      (isolated
        ? createInitializedBamlWorker(codeRef.current)
        : getBamlWorker(codeRef.current)
      )
        .then((worker) => {
          const onMessage = (e: MessageEvent) => {
            if (e.data?.type === 'lspDiagnostics') {
              applyDiagnostics(e.data.params as LspPublish);
            } else if (
              nudgedRef.current &&
              !readyFiredRef.current &&
              (e.data?.type === 'controlFlowGraphResult' ||
                e.data?.notification?.type === 'updateProject')
            ) {
              // First graph/project state for THIS mount's code has arrived
              // (gated on the nudge so we don't latch the previous example's
              // stale state) — lift the pane veil and notify callers.
              readyFiredRef.current = true;
              setReady(true);
              onReadyRef.current?.();
            }
          };
          worker.addEventListener('message', onMessage);
          workerRef.current = worker;
          msgHandlerRef.current = onMessage;

          const p = new WorkerRuntimePort(worker);
          portRef.current = p;
          setPort(p);
          setVersion((v) => v + 1);
          // The shared worker still holds the previous example's project. Push
          // this mount's code so it re-evaluates to the right graph; the panel
          // subscribes on the next render, so a short delay avoids the race.
          setTimeout(() => {
            p.postMessage({
              files: { 'baml_src/main.baml': codeRef.current },
              type: 'filesChanged',
            });
            nudgedRef.current = true;
            // Seed the panel selection from the initial caret (best-effort —
            // the project may still be collecting; the first click corrects it).
            postCursor();
          }, 120);
          // Safety net: never leave the veil up forever if no graph arrives.
          setTimeout(() => setReady(true), 6000);
        })
        .catch(() => {
          setFailed(true);
          setReady(true);
        });
    },
    [applyDiagnostics, postCursor, highlightLines, isolated],
  );

  const onChange = useCallback((value?: string) => {
    const next = value ?? '';
    codeRef.current = next;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    // 200ms debounce: rapid keystrokes must coalesce or the runtime panics.
    debounceRef.current = setTimeout(() => {
      portRef.current?.postMessage({
        files: { 'baml_src/main.baml': codeRef.current },
        type: 'filesChanged',
      });
    }, 200);
  }, []);

  // On unmount (e.g. switching workflow tabs) detach this mount's worker
  // listener and dispose its port, so the shared worker isn't left serving
  // zombie panels that slow every later switch. Ref-cleanup, no useEffect.
  const teardownRef = useCallback((node: HTMLDivElement | null) => {
    if (!node) return undefined;
    return () => {
      try {
        portRef.current?.dispose();
      } catch {
        /* already gone */
      }
      portRef.current = null;
      const w = workerRef.current;
      const h = msgHandlerRef.current;
      if (w && h) w.removeEventListener('message', h);
      // A dedicated (isolated) worker is owned by this mount — terminate it so
      // it doesn't leak. The page-shared worker is left running for reuse.
      if (w && ownsWorkerRef.current) {
        try {
          w.terminate();
        } catch {
          /* already gone */
        }
      }
      workerRef.current = null;
      msgHandlerRef.current = null;
    };
  }, []);

  return (
    <div
      className={`baml-playground-root l2-live${fill ? ' l2-live--fill' : ''}`}
      data-theme={codeTheme.dark ? 'dark' : 'light'}
      ref={teardownRef}
    >
      <ResizablePanelGroup direction="horizontal">
        {/* `nokey` opts the editor out of React Flow's global key capture
            (the graph pane grabs Space/Backspace/etc otherwise). */}
        <ResizablePanel
          className="l2-live-editor nokey"
          defaultSize={56}
          minSize={28}
        >
          <div
            style={{ display: 'flex', flexDirection: 'column', height: '100%' }}
          >
            {filename ? (
              <div className="l2-code-head">
                <span aria-hidden className="l2-code-dots">
                  <i />
                  <i />
                  <i />
                </span>
                <span className="l2-code-name font-mono">{filename}</span>
              </div>
            ) : null}
            <div style={{ flex: 1, minHeight: 0 }}>
              <Editor
                beforeMount={beforeMount}
                defaultLanguage="baml"
                defaultValue={initialCode}
                height="100%"
                onChange={onChange}
                onMount={onMount}
                options={{
                  automaticLayout: true,
                  // Hover/suggest widgets render in a viewport-fixed overlay so
                  // they escape the editor's overflow-clipped frame instead of
                  // landing under the file header.
                  fixedOverflowWidgets: true,
                  fontFamily: 'var(--font-geist-mono), ui-monospace, monospace',
                  fontSize: 13,
                  guides: { indentation: false },
                  hideCursorInOverviewRuler: true,
                  hover: { above: false },
                  lineNumbers: 'on',
                  minimap: { enabled: false },
                  overviewRulerLanes: 0,
                  padding: { bottom: 12, top: 12 },
                  renderLineHighlight: 'line',
                  scrollBeyondLastLine: false,
                  scrollbar: {
                    // Don't trap page scroll when the cursor rests on the editor.
                    alwaysConsumeMouseWheel: false,
                    horizontalSliderSize: 6,
                    verticalSliderSize: 6,
                  },
                  tabSize: 2,
                  wordWrap: 'on',
                }}
                theme={codeTheme.monaco}
              />
            </div>
          </div>
        </ResizablePanel>
        <ResizableHandle className="l2-live-splitter" withHandle />
        <ResizablePanel className="l2-live-panel" defaultSize={44} minSize={22}>
          <div className="l2-live-pane">
            {failed ? (
              <div className="l2-live-loading">runtime failed to start</div>
            ) : port ? (
              <ExecutionPanel
                argsByFunction={argsByFunction}
                connectionVersion={version}
                initialArgsJson="{}"
                initialFunctionName={initialFunction}
                initialSidebarOpen={initialSidebarOpen}
                initialTab={initialTab}
                onNavigateToSource={onNavigateToSource}
                port={port}
              />
            ) : (
              <div className="l2-live-loading">starting runtime…</div>
            )}
            {/* Veil over the graph pane only (never the editor) until ready. */}
            {!failed && !ready ? (
              <div aria-live="polite" className="l2-live-veil">
                <span aria-hidden className="l2-live-spinner" />
                {loadingLabel}
              </div>
            ) : null}
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
