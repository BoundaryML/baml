'use client';

import Editor, { type BeforeMount, type OnMount } from '@monaco-editor/react';
import { useCallback, useRef, useState } from 'react';
import { useCodeTheme } from '../_lib/code-theme';
import { registerBaml } from '../_lib/baml-monarch';
import {
  type CellHandle,
  type LspDiagnostic,
  type RunResult,
  registerCell,
} from './runtime';

type EditorInstance = Parameters<OnMount>[0];
type MonacoNs = Parameters<OnMount>[1];
type DecorationsCollection = ReturnType<
  EditorInstance['createDecorationsCollection']
>;
type TextModel = NonNullable<ReturnType<EditorInstance['getModel']>>;

let cellCounter = 0;

// One global CodeLens provider for all editors; routes each model to its cell.
const modelHandles = new Map<unknown, CellHandle>();
let lensProviderRegistered = false;
let fireInlayRefresh: (() => void) | null = null;
const RUN_CMD = 'learn2.runLens';

// Per-cell handle + output setters so the global lens command can run a test
// and route the result to the right editor's inline output box.
const cellHandles = new Map<string, CellHandle>();
type RunStatus = 'running' | 'pass' | 'fail' | 'error';
interface RunLine {
  id: number;
  testName: string;
  status: RunStatus;
  durationMs?: number;
  summary?: string;
  error?: string;
}

interface CellOutput {
  start: (testName: string) => number;
  finish: (id: number, result: RunResult) => void;
}
const cellOutput = new Map<string, CellOutput>();

function extractCellId(projectPath?: string): string | undefined {
  if (!projectPath) return undefined;
  const parts = projectPath.split('/').filter(Boolean);
  const idx = parts.indexOf('workspace');
  return idx >= 0 ? parts[idx + 1] : parts[parts.length - 1];
}

function toStatus(outcome?: string): RunStatus {
  if (outcome === 'fail') return 'fail';
  if (outcome === 'error') return 'error';
  return 'pass';
}

/** Parse a run result (testing.TestReport / testing.TestSetReport). */
function parseReport(result: RunResult): Partial<RunLine> {
  if (!result.ok) return { status: 'error', error: result.error };
  const v = result.value as
    | {
        outcome?: string;
        runs?: Array<{ duration_ms?: number; outcome?: string }>;
        results?: unknown[];
        passed?: number;
        total?: number;
      }
    | null
    | undefined;
  // TestSetReport
  if (v && Array.isArray(v.results)) {
    return {
      status: toStatus(v.outcome),
      summary:
        typeof v.passed === 'number' && typeof v.total === 'number'
          ? `${v.passed}/${v.total} passed`
          : undefined,
    };
  }
  // TestReport
  const runs = v?.runs ?? [];
  const durationMs = runs.reduce((s, r) => s + (r.duration_ms ?? 0), 0);
  const status = toStatus(
    v?.outcome ??
      (runs.some((r) => r.outcome && r.outcome !== 'pass') ? 'fail' : 'pass'),
  );
  return {
    status,
    durationMs,
    summary: runs.length > 1 ? `${runs.length} runs` : undefined,
  };
}

function statusIcon(status: RunStatus): string {
  if (status === 'pass') return '✓';
  if (status === 'fail') return '✗';
  if (status === 'error') return '!';
  return '…';
}

/** Flatten an LSP Hover `contents` (MarkupContent / MarkedString / arrays) to markdown. */
function hoverToMarkdown(contents: unknown): string {
  if (contents == null) return '';
  if (typeof contents === 'string') return contents;
  if (Array.isArray(contents)) {
    return contents.map(hoverToMarkdown).filter(Boolean).join('\n\n');
  }
  const c = contents as { value?: unknown; language?: string };
  if (typeof c.value === 'string') {
    return c.language ? `\`\`\`${c.language}\n${c.value}\n\`\`\`` : c.value;
  }
  return '';
}

function registerLensProvider(monaco: MonacoNs) {
  if (lensProviderRegistered) return;
  lensProviderRegistered = true;

  monaco.editor.registerCommand(
    RUN_CMD,
    async (_accessor: unknown, arg?: unknown) => {
      const a = (arg ?? {}) as { projectPath?: string; functionName?: string };
      const cellId = extractCellId(a.projectPath);
      const testName = a.functionName;
      if (!cellId || !testName) return;
      const handle = cellHandles.get(cellId);
      const out = cellOutput.get(cellId);
      if (!handle || !out) return;
      const runId = out.start(testName);
      const result = await handle.runTest(testName);
      out.finish(runId, result);
    },
  );

  monaco.languages.registerCodeLensProvider('baml', {
    provideCodeLenses: async (model: TextModel) => {
      const handle = modelHandles.get(model);
      if (!handle) return { lenses: [], dispose() {} };
      const items = await handle.requestCodeLens();
      return {
        lenses: items
          // The runtime emits an "▶ Open 🐑 Playground" lens on every function
          // plus "▶ Run test" / "▶ Run testset" lenses on tests. Inline deck
          // editors have no playground panel to open, so keep only the test
          // runners and drop the Playground lens.
          .filter((l) => l.command && !/playground/i.test(l.command.title))
          .map((l, i) => ({
            range: new monaco.Range(
              l.range.start.line + 1,
              l.range.start.character + 1,
              l.range.end.line + 1,
              l.range.end.character + 1,
            ),
            id: `lens-${i}`,
            command: {
              id: RUN_CMD,
              // biome-ignore lint/style/noNonNullAssertion: filtered above
              title: l.command!.title,
              // biome-ignore lint/style/noNonNullAssertion: filtered above
              arguments: l.command!.arguments,
            },
          })),
        dispose() {},
      };
    },
  });

  monaco.languages.registerHoverProvider('baml', {
    provideHover: async (
      model: TextModel,
      position: { lineNumber: number; column: number },
    ) => {
      const handle = modelHandles.get(model);
      if (!handle) return null;
      const hover = await handle.hover(
        position.lineNumber - 1,
        position.column - 1,
      );
      const value = hover ? hoverToMarkdown(hover.contents) : '';
      if (!value) return null;
      const range = hover?.range
        ? new monaco.Range(
            hover.range.start.line + 1,
            hover.range.start.character + 1,
            hover.range.end.line + 1,
            hover.range.end.character + 1,
          )
        : undefined;
      return { contents: [{ value }], range };
    },
  });

  const inlayEmitter = new monaco.Emitter<void>();
  fireInlayRefresh = () => inlayEmitter.fire();
  monaco.languages.registerInlayHintsProvider('baml', {
    onDidChangeInlayHints: inlayEmitter.event,
    provideInlayHints: async (
      model: TextModel,
      range: {
        startLineNumber: number;
        startColumn: number;
        endLineNumber: number;
        endColumn: number;
      },
    ) => {
      const handle = modelHandles.get(model);
      if (!handle) return { hints: [], dispose() {} };
      // Request the WHOLE document range (not just Monaco's visible range) so
      // range-filtering can't drop hints for small editors.
      const lastLine = model.getLineCount();
      const hints = await handle.inlayHints({
        start: { line: 0, character: 0 },
        end: {
          line: lastLine - 1,
          character: model.getLineMaxColumn(lastLine) - 1,
        },
      });
      return {
        hints: hints.map((h) => ({
          position: {
            lineNumber: h.position.line + 1,
            column: h.position.character + 1,
          },
          label:
            typeof h.label === 'string'
              ? h.label
              : h.label.map((p) => ({ label: p.value ?? p.label ?? '' })),
          paddingLeft: h.paddingLeft,
          paddingRight: h.paddingRight,
        })),
        dispose() {},
      };
    },
  });

  monaco.languages.registerCompletionItemProvider('baml', {
    triggerCharacters: ['.'],
    provideCompletionItems: async (
      model: TextModel,
      position: { lineNumber: number; column: number },
    ) => {
      const handle = modelHandles.get(model);
      if (!handle) return { suggestions: [] };
      const items = await handle.completion(
        position.lineNumber - 1,
        position.column - 1,
      );
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      return {
        suggestions: items.map((it) => {
          const label =
            typeof it.label === 'string' ? it.label : it.label.label;
          const documentation =
            typeof it.documentation === 'string'
              ? it.documentation
              : it.documentation?.value;
          return {
            label,
            kind: completionKind(monaco, it.kind),
            insertText: it.insertText ?? it.textEdit?.newText ?? label,
            detail: it.detail,
            documentation,
            sortText: it.sortText,
            filterText: it.filterText,
            range,
          };
        }),
      };
    },
  });
}

function completionKind(monaco: MonacoNs, kind?: number) {
  const K = monaco.languages.CompletionItemKind;
  switch (kind) {
    case 2:
      return K.Method;
    case 3:
      return K.Function;
    case 4:
      return K.Constructor;
    case 5:
      return K.Field;
    case 6:
      return K.Variable;
    case 7:
      return K.Class;
    case 8:
      return K.Interface;
    case 9:
      return K.Module;
    case 10:
      return K.Property;
    case 13:
      return K.Enum;
    case 14:
      return K.Keyword;
    case 15:
      return K.Snippet;
    case 20:
      return K.EnumMember;
    case 21:
      return K.Constant;
    case 22:
      return K.Struct;
    case 25:
      return K.TypeParameter;
    default:
      return K.Text;
  }
}

function markerSeverity(monaco: MonacoNs, s?: number) {
  if (s === 2) return monaco.MarkerSeverity.Warning;
  if (s === 3 || s === 4) return monaco.MarkerSeverity.Info;
  return monaco.MarkerSeverity.Error;
}
function severityKind(s?: number): 'error' | 'warning' | 'info' {
  if (s === 2) return 'warning';
  if (s === 3 || s === 4) return 'info';
  return 'error';
}

export interface BamlEditorProps {
  initialCode: string;
  /** Fixed height (px or any CSS size). Omit to auto-size to the code. */
  height?: number | string;
  readOnly?: boolean;
  /** Filename shown in a header bar, matching the deck's static code blocks. */
  filename?: string;
  /** Cap for auto-size; taller code scrolls inside the editor (default 440). */
  maxHeight?: number;
  /** 1-based lines to softly emphasise (whole-line tint, like BamlCode). */
  highlightLines?: number[];
  /**
   * Show a small "try editing me" nudge over the editor (desktop only, hidden
   * on touch). Dismisses the first time the editor is focused or edited.
   */
  editHint?: boolean;
  /** Show the ▶ Run codelenses (default true). Embeds that defer running to
   *  an expanded playground pass false. */
  codeLens?: boolean;
}

/**
 * A single BAML editor that shares the multi-editor runtime. Render as many as
 * you like on a page — they each get an isolated project + their own diagnostics.
 */
export function BamlEditor({
  initialCode,
  height,
  readOnly,
  filename,
  maxHeight = 440,
  highlightLines,
  editHint = false,
  codeLens = true,
}: BamlEditorProps) {
  const idRef = useRef<string>('');
  if (!idRef.current) idRef.current = `cell${cellCounter++}`;

  // Monaco theme is chosen by the page (CodeThemeProvider). Held in a ref so
  // the stable onMount callback can read it without re-creating.
  const codeTheme = useCodeTheme();
  const monacoThemeRef = useRef(codeTheme.monaco);
  monacoThemeRef.current = codeTheme.monaco;

  const [runs, setRuns] = useState<RunLine[]>([]);
  const runIdRef = useRef(0);

  // Auto-grow to fit the code (capped at maxHeight — taller snippets scroll
  // inside the editor instead of blowing out the slide) unless the caller pins a
  // height. Seeded from the line count so first paint is close, then corrected
  // by Monaco's content-size event (wired in onMount — a library callback).
  const autoSize = height == null;
  const [measured, setMeasured] = useState(() =>
    Math.min(maxHeight, initialCode.split('\n').length * 20 + 20),
  );
  const boxHeight = height ?? measured;

  // Header "Run" button: runs every test/testset in the cell via the same
  // lens metadata the codelenses use. Heuristic visibility: only cells whose
  // shipped code contains a test block get the button.
  const hasTests = /\btest(set)?\s+"/.test(initialCode);
  const [running, setRunning] = useState(false);
  const [hintOff, setHintOff] = useState(false);
  const runningRef = useRef(false);
  const runAll = useCallback(async () => {
    const handle = handleRef.current;
    const out = cellOutput.get(idRef.current);
    if (!handle || !out || runningRef.current) return;
    runningRef.current = true;
    setRunning(true);
    try {
      const items = await handle.requestCodeLens();
      const runnable = items.filter(
        (l) => l.command && !/playground/i.test(l.command.title),
      );
      // Prefer testset lenses (they run their member tests); fall back to
      // bare test lenses for cells with top-level tests only.
      const sets = runnable.filter((l) =>
        /testset/i.test(l.command?.title ?? ''),
      );
      const picks = sets.length > 0 ? sets : runnable;
      for (const l of picks) {
        const a = (l.command?.arguments?.[0] ?? {}) as {
          functionName?: string;
          testsetName?: string;
          testName?: string;
          name?: string;
        };
        // Testset lenses carry their name under a different key than test
        // lenses, so try each; runTest() routes test vs testset by the tree.
        const name = a.functionName ?? a.testsetName ?? a.testName ?? a.name;
        if (!name) continue;
        const runId = out.start(name);
        const result = await handle.runTest(name);
        out.finish(runId, result);
      }
    } finally {
      runningRef.current = false;
      setRunning(false);
    }
  }, []);

  const handleRef = useRef<CellHandle | null>(null);
  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoNs | null>(null);
  const lensRef = useRef<DecorationsCollection | null>(null);
  const codeRef = useRef(initialCode);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const applyDiagnostics = useCallback((diags: LspDiagnostic[]) => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    if (!editor || !monaco || !model) return;

    monaco.editor.setModelMarkers(
      model,
      'baml',
      diags.map((d) => ({
        startLineNumber: d.range.start.line + 1,
        startColumn: d.range.start.character + 1,
        endLineNumber: d.range.end.line + 1,
        endColumn: d.range.end.character + 1,
        message: d.message,
        severity: markerSeverity(monaco, d.severity),
      })),
    );

    try {
      if (!lensRef.current) {
        lensRef.current = editor.createDecorationsCollection();
      }
      lensRef.current.set(
        diags.map((d) => {
          const line = d.range.start.line + 1;
          const kind = severityKind(d.severity);
          const endCol = model.getLineMaxColumn(line);
          const inline =
            d.message.length > 100 ? `${d.message.slice(0, 100)}…` : d.message;
          return {
            range: new monaco.Range(line, 1, line, endCol),
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
          };
        }),
      );
    } catch {
      // decoration application is best-effort
    }
    // Project (re)evaluated → refresh inlay hints, which may have returned
    // empty before the project's types were ready.
    fireInlayRefresh?.();
  }, []);

  const beforeMount: BeforeMount = useCallback((monaco) => {
    registerBaml(monaco);
  }, []);

  const onMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      if (editHint) {
        editor.onDidFocusEditorText(() => setHintOff(true));
      }
      monaco.editor.setTheme(monacoThemeRef.current);
      const handle = registerCell(idRef.current, codeRef.current);
      handleRef.current = handle;
      handle.onDiagnostics(applyDiagnostics);
      cellHandles.set(idRef.current, handle);
      cellOutput.set(idRef.current, {
        start(testName) {
          const id = (runIdRef.current += 1);
          setRuns((prev) => [...prev, { id, testName, status: 'running' }]);
          return id;
        },
        finish(id, result) {
          const parsed = parseReport(result);
          setRuns((prev) =>
            prev.map((r) => (r.id === id ? { ...r, ...parsed } : r)),
          );
        },
      });
      const model = editor.getModel();
      if (model) modelHandles.set(model, handle);
      registerLensProvider(monaco);
      // Showcase highlights: a static whole-line tint on the initial code.
      // Deliberately not tracked across edits — it marks the shipped snippet.
      if (highlightLines?.length) {
        editor.createDecorationsCollection(
          highlightLines.map((line) => ({
            range: new monaco.Range(line, 1, line, 1),
            options: { isWholeLine: true, className: 'l2-ed-hl' },
          })),
        );
      }
      if (autoSize) {
        const syncHeight = () =>
          setMeasured(
            Math.min(maxHeight, Math.max(40, editor.getContentHeight())),
          );
        editor.onDidContentSizeChange(syncHeight);
        syncHeight();
      }
    },
    [applyDiagnostics, autoSize, maxHeight, highlightLines, editHint],
  );

  const onChange = useCallback((value?: string) => {
    const next = value ?? '';
    codeRef.current = next;
    setHintOff(true);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      handleRef.current?.updateCode(codeRef.current);
    }, 200);
  }, []);

  // `nokey` opts the editor out of React Flow's global key capture
  // (the playground graph grabs Space/Backspace/etc otherwise).
  return (
    <div className="l2-bamled-wrap nokey">
      {editHint && !hintOff ? (
        <span className="l2-edit-hint font-mono" aria-hidden>
          <span className="l2-edit-hint-emoji">✏️</span>
          try editing me
        </span>
      ) : null}
      <div className="l2-bamled-frame">
        {filename || (hasTests && codeLens) ? (
          <div
            className={`l2-code-head${
              filename?.toLowerCase().endsWith('.baml')
                ? ' l2-code-head--baml'
                : ''
            }`}
          >
            <span className="l2-code-dots" aria-hidden>
              <i />
              <i />
              <i />
            </span>
            {filename ? (
              <span className="l2-code-name font-mono">{filename}</span>
            ) : null}
            {hasTests && codeLens ? (
              <button
                type="button"
                className="l2-run-btn font-mono"
                onClick={runAll}
                disabled={running}
              >
                {running ? 'running…' : '▶ Run'}
              </button>
            ) : null}
          </div>
        ) : null}
        <div className="l2-bamled" style={{ height: boxHeight }}>
          <Editor
            defaultLanguage="baml"
            defaultValue={initialCode}
            theme={codeTheme.monaco}
            beforeMount={beforeMount}
            onMount={onMount}
            onChange={onChange}
            height="100%"
            options={{
              codeLens,
              minimap: { enabled: false },
              fontSize: 13,
              fontFamily: 'var(--font-geist-mono), ui-monospace, monospace',
              lineNumbers: 'on',
              scrollBeyondLastLine: false,
              renderLineHighlight: 'line',
              overviewRulerLanes: 0,
              hideCursorInOverviewRuler: true,
              scrollbar: {
                verticalSliderSize: 6,
                horizontalSliderSize: 6,
                // On scroll pages, let the wheel pass through to the page
                // once the editor has nothing left to scroll.
                alwaysConsumeMouseWheel: false,
              },
              padding: { top: 10, bottom: 10 },
              tabSize: 2,
              wordWrap: 'on',
              automaticLayout: true,
              // Render hover/suggest widgets in a viewport-fixed overlay so
              // they escape the editor's overflow-clipped frame instead of
              // landing under the file header. Safe because no ancestor sets a
              // transform/filter (which would re-anchor the fixed widget).
              fixedOverflowWidgets: true,
              guides: { indentation: false },
              hover: { above: false },
              inlayHints: { enabled: 'on' },
              readOnly: !!readOnly,
            }}
          />
        </div>
        {runs.length > 0 ? (
          <div className="l2-runs">
            <div className="l2-runs-head font-mono">
              <span className="l2-runs-title">results</span>
              <span className="l2-runs-summary">
                {runs.some((r) => r.status === 'running')
                  ? 'running…'
                  : `${runs.filter((r) => r.status === 'pass').length}/${runs.length} passed`}
              </span>
              <button
                type="button"
                className="l2-runs-clear"
                onClick={() => setRuns([])}
              >
                clear
              </button>
            </div>
            {runs.map((r) => (
              <div key={r.id} className={`l2-runs-row l2-runs--${r.status}`}>
                <span className="l2-runs-check">{statusIcon(r.status)}</span>
                <span className="l2-runs-name">{r.testName}</span>
                {r.status === 'running' ? (
                  <span className="l2-runs-meta">running…</span>
                ) : (
                  <>
                    {r.summary ? (
                      <span className="l2-runs-meta">{r.summary}</span>
                    ) : null}
                    {r.durationMs != null ? (
                      <span className="l2-runs-meta">{r.durationMs}ms</span>
                    ) : null}
                    {r.error ? (
                      <span className="l2-runs-err">{r.error}</span>
                    ) : null}
                  </>
                )}
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
