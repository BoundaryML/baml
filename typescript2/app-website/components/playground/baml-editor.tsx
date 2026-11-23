'use client';

import Editor, { type BeforeMount, type OnMount } from '@monaco-editor/react';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  BAML_THEME,
  registerBaml,
} from '@/app/(home)/bamlcode/_lib/baml-monarch';
import { MONO_FONT } from '@/app/(home)/bamlcode/_lib/code-palette';
import {
  type CellHandle,
  createCell,
  type LspDiagnostic,
  type RunResult,
  shortName,
} from './runtime';

// A live BAML editor: the code is checked by the real compiler as you type, with its errors after
// the lines they are on, hovers, inlay hints, and completions, and Run executes it in the page.
// Each editor has its own runtime (./runtime.ts), a worker, so a page keeps awake only the editors
// near the window (./live-code.tsx). An editor that is asleep keeps its code and its output, and
// shows no new diagnostics until it wakes.
//
// Restored from the previous site's learn2/_editor/baml-editor.tsx, in this site's colours. What
// changed: Run also calls `main` and shows what it returned, where the old editor ran tests only.

type EditorInstance = Parameters<OnMount>[0];
type MonacoNs = Parameters<OnMount>[1];
type Decorations = ReturnType<EditorInstance['createDecorationsCollection']>;
type TextModel = NonNullable<ReturnType<EditorInstance['getModel']>>;
/** A position in Monaco's terms, which count lines and columns from 1. */
type Caret = { lineNumber: number; column: number };

type Status = 'running' | 'pass' | 'fail' | 'error';
/** One line of output under the editor: a call to `main`, or a test. */
interface Line {
  id: number;
  name: string;
  status: Status;
  /** What `main` returned, printed. */
  value?: string;
  detail?: string;
  /** A testset's tests, each with whether it passed. */
  tests?: { name: string; passed: boolean }[];
}
interface Output {
  start(name: string): number;
  finish(id: number, line: Partial<Line>): void;
}

// Monaco's providers and its Run command are registered once for the language, so each looks up
// the cell, and the output, that belong to the model it was called for.
const byModel = new Map<
  unknown,
  { handle: CellHandle | null; output: Output }
>();
let registered = false;
let refreshInlayHints: (() => void) | null = null;
let refreshLenses: (() => void) | null = null;
const RUN = 'baml.runLens';

const kind = (severity?: number): 'error' | 'warning' | 'info' =>
  severity === 2
    ? 'warning'
    : severity === 3 || severity === 4
      ? 'info'
      : 'error';

/** An LSP hover's contents as Markdown, whichever of its shapes it came in. */
function markdown(contents: unknown): string {
  if (contents == null) return '';
  if (typeof contents === 'string') return contents;
  if (Array.isArray(contents)) {
    return contents.map(markdown).filter(Boolean).join('\n\n');
  }
  const c = contents as { value?: unknown; language?: string };
  if (typeof c.value !== 'string') return '';
  return c.language ? `\`\`\`${c.language}\n${c.value}\n\`\`\`` : c.value;
}

/** A returned value, printed as BAML's own literals would read. */
function print(value: unknown, depth = 0): string {
  const pad = '  '.repeat(depth + 1);
  const end = '  '.repeat(depth);
  if (value === null || value === undefined) return 'null';
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'bigint' || typeof value === 'number') {
    return String(value);
  }
  if (typeof value === 'boolean') return String(value);
  if (Array.isArray(value)) {
    if (value.length === 0) return '[]';
    return `[\n${value.map((v) => `${pad}${print(v, depth + 1)},`).join('\n')}\n${end}]`;
  }
  const entries =
    value instanceof Map
      ? [...value.entries()]
      : Object.entries(value as Record<string, unknown>);
  // `$baml` carries the value's type: a class is printed with its name.
  const tag = entries.find(([k]) => k === '$baml')?.[1] as
    | { type?: string }
    | undefined;
  const fields = entries.filter(([k]) => k !== '$baml');
  const name = tag?.type ? `${tag.type.split('.').at(-1)} ` : '';
  if (fields.length === 0) return `${name}{}`;
  return `${name}{\n${fields.map(([k, v]) => `${pad}${String(k)}: ${print(v, depth + 1)},`).join('\n')}\n${end}}`;
}

/** A test's or testset's report, as an output line. */
function report(result: RunResult): Partial<Line> {
  if (!result.ok && result.value == null) {
    return { detail: result.error, status: 'error' };
  }
  const v = result.value as {
    outcome?: string;
    passed?: number;
    total?: number;
    results?: unknown[];
    runs?: { duration_ms?: number; outcome?: string }[];
  } | null;
  if (v && Array.isArray(v.results)) {
    return {
      detail: `${v.passed} of ${v.total} passed`,
      status: v.outcome === 'pass' ? 'pass' : 'fail',
      tests: v.results as { name: string; passed: boolean }[],
    };
  }
  const runs = v?.runs ?? [];
  const failed =
    v?.outcome != null
      ? v.outcome !== 'pass'
      : runs.some((r) => r.outcome && r.outcome !== 'pass');
  return { status: failed ? 'fail' : 'pass' };
}

/** The name a code lens runs: tests and testsets carry it under different keys. */
function lensName(argument: unknown): string | undefined {
  const a = (argument ?? {}) as Record<string, string | undefined>;
  return a.functionName ?? a.testsetName ?? a.testName ?? a.name;
}

/** Registers the language's providers with Monaco, once: the Run lenses, hovers, inlay hints, and
 *  completions, each answered by the cell that owns the model. */
function registerProviders(monaco: MonacoNs) {
  if (registered) return;
  registered = true;

  monaco.editor.registerCommand(
    RUN,
    async (_accessor: unknown, argument?: unknown, uri?: string) => {
      const model = monaco.editor
        .getModels()
        .find((m: TextModel) => m.uri.toString() === uri);
      const cell = byModel.get(model);
      const name = lensName(argument);
      if (!cell?.handle || !name) return;
      const line = cell.output.start(name);
      cell.output.finish(line, report(await cell.handle.runTest(name)));
    },
  );

  // Monaco asks for a model's lenses when it opens, which is before its cell has compiled
  // anything. This tells it to ask again once the compiler has answered.
  const lensesChanged = new monaco.Emitter<void>();
  refreshLenses = () => lensesChanged.fire();
  monaco.languages.registerCodeLensProvider('baml', {
    onDidChange: lensesChanged.event,
    provideCodeLenses: async (model: TextModel) => {
      const handle = byModel.get(model)?.handle;
      const lenses = handle ? await handle.codeLenses() : [];
      return {
        dispose() {},
        lenses: lenses
          // The runtime also offers to open its playground on every function. There is no
          // playground here, so only the lenses that run a test are kept.
          .filter((l) => l.command && !/playground/i.test(l.command.title))
          .map((l, i) => ({
            command: {
              // Every cell's project has the same path, so the lens also says which editor's
              // model it is in.
              arguments: [l.command?.arguments?.[0], model.uri.toString()],
              id: RUN,
              title: l.command?.title ?? '',
            },
            id: `lens-${i}`,
            range: new monaco.Range(
              l.range.start.line + 1,
              l.range.start.character + 1,
              l.range.end.line + 1,
              l.range.end.character + 1,
            ),
          })),
      };
    },
  });

  monaco.languages.registerHoverProvider('baml', {
    provideHover: async (model: TextModel, at: Caret) => {
      const hover = await byModel
        .get(model)
        ?.handle?.hover({ character: at.column - 1, line: at.lineNumber - 1 });
      const value = hover ? markdown(hover.contents) : '';
      if (!value) return null;
      return {
        contents: [{ value }],
        range: hover?.range
          ? new monaco.Range(
              hover.range.start.line + 1,
              hover.range.start.character + 1,
              hover.range.end.line + 1,
              hover.range.end.character + 1,
            )
          : undefined,
      };
    },
  });

  const changed = new monaco.Emitter<void>();
  refreshInlayHints = () => changed.fire();
  monaco.languages.registerInlayHintsProvider('baml', {
    onDidChangeInlayHints: changed.event,
    provideInlayHints: async (model: TextModel) => {
      const handle = byModel.get(model)?.handle;
      if (!handle) return { dispose() {}, hints: [] };
      // The whole document is asked for, so a short editor's hints are never filtered out.
      const last = model.getLineCount();
      const hints = await handle.inlayHints({
        end: { character: model.getLineMaxColumn(last) - 1, line: last - 1 },
        start: { character: 0, line: 0 },
      });
      return {
        dispose() {},
        hints: hints.map((h) => ({
          label:
            typeof h.label === 'string'
              ? h.label
              : h.label.map((p) => ({ label: p.value ?? p.label ?? '' })),
          paddingLeft: h.paddingLeft,
          paddingRight: h.paddingRight,
          position: {
            column: h.position.character + 1,
            lineNumber: h.position.line + 1,
          },
        })),
      };
    },
  });

  // LSP completion kinds, by number, as Monaco names them.
  const K = monaco.languages.CompletionItemKind;
  const kinds: Record<number, number> = {
    2: K.Method,
    3: K.Function,
    4: K.Constructor,
    5: K.Field,
    6: K.Variable,
    7: K.Class,
    8: K.Interface,
    9: K.Module,
    10: K.Property,
    13: K.Enum,
    14: K.Keyword,
    15: K.Snippet,
    20: K.EnumMember,
    21: K.Constant,
    22: K.Struct,
    25: K.TypeParameter,
  };
  monaco.languages.registerCompletionItemProvider('baml', {
    provideCompletionItems: async (model: TextModel, at: Caret) => {
      const handle = byModel.get(model)?.handle;
      if (!handle) return { suggestions: [] };
      const items = await handle.completions({
        character: at.column - 1,
        line: at.lineNumber - 1,
      });
      const word = model.getWordUntilPosition(at);
      const range = {
        endColumn: word.endColumn,
        endLineNumber: at.lineNumber,
        startColumn: word.startColumn,
        startLineNumber: at.lineNumber,
      };
      return {
        suggestions: items.map((item) => {
          const label =
            typeof item.label === 'string' ? item.label : item.label.label;
          return {
            detail: item.detail,
            documentation:
              typeof item.documentation === 'string'
                ? item.documentation
                : item.documentation?.value,
            filterText: item.filterText,
            insertText: item.insertText ?? item.textEdit?.newText ?? label,
            kind: kinds[item.kind ?? 0] ?? K.Text,
            label,
            range,
            sortText: item.sortText,
          };
        }),
      };
    },
    triggerCharacters: ['.'],
  });
}

const MARK: Record<Status, { sign: string; tone: string }> = {
  error: { sign: '!', tone: 'text-[#f0787c]' },
  fail: { sign: '✗', tone: 'text-[#f0787c]' },
  pass: { sign: '✓', tone: 'text-(--color-pasture-200)' },
  running: { sign: '…', tone: 'text-muted' },
};

export interface BamlEditorProps {
  /** The code the editor starts with. */
  code: string;
  /** The name shown above the code, such as "errors.baml". */
  filename?: string;
  /** Whether the sample can be run here. A sample that calls a model can't: it needs an API key. */
  runnable?: boolean;
  /** The tallest the editor grows before its code scrolls inside it, in pixels. */
  maxHeight?: number;
  /** Whether the editor's runtime is running. Asleep, it has no worker: its code and output stay,
   *  and it is checked again when it wakes. */
  awake?: boolean;
}

/** A live BAML editor with its output under it. */
export default function BamlEditor({
  code,
  filename,
  runnable = true,
  maxHeight = 900,
  awake = true,
}: BamlEditorProps) {
  const [lines, setLines] = useState<Line[]>([]);
  const [running, setRunning] = useState(false);
  // The editor is as tall as its code. The first height is worked out from the line count, so the
  // page barely moves when Monaco reports the real one.
  const [height, setHeight] = useState(() =>
    Math.min(maxHeight, code.split('\n').length * 20 + 24),
  );

  const handle = useRef<CellHandle | null>(null);
  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoNs | null>(null);
  const messages = useRef<Decorations | null>(null);
  const latest = useRef(code);
  const lineId = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const busy = useRef(false);

  const output = useRef<Output>({
    finish(line, result) {
      setLines((all) =>
        all.map((l) => (l.id === line ? { ...l, ...result } : l)),
      );
    },
    start(name) {
      lineId.current += 1;
      const line = lineId.current;
      setLines((all) => [...all, { id: line, name, status: 'running' }]);
      return line;
    },
  });

  /** Runs the sample: `main`, if it has one, and then each of its tests. */
  const run = useCallback(async () => {
    const cell = handle.current;
    if (!cell || busy.current) return;
    busy.current = true;
    setRunning(true);
    setLines([]);
    const out = output.current;
    try {
      // An edit still waiting out its 200ms is sent now, so Run never executes the code before it.
      if (timer.current) {
        clearTimeout(timer.current);
        timer.current = null;
        cell.update(latest.current);
      }
      const error = await cell.check();
      if (error) {
        out.finish(out.start('does not compile'), {
          detail: `line ${error.range.start.line + 1}: ${error.message}`,
          status: 'error',
        });
        return;
      }
      if (/^function\s+main\s*\(/m.test(latest.current)) {
        const line = out.start('main()');
        const result = await cell.runFunction('main');
        out.finish(
          line,
          result.ok
            ? { status: 'pass', value: print(result.value) }
            : { detail: result.error, status: 'error' },
        );
      }
      for (const test of await cell.tests()) {
        const line = out.start(shortName(test));
        out.finish(line, report(await cell.runTest(test)));
      }
    } finally {
      busy.current = false;
      setRunning(false);
    }
  }, []);

  /** Draws the compiler's diagnostics: a squiggle with a hover, and the message after the line. */
  const showDiagnostics = useCallback((diagnostics: LspDiagnostic[]) => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    if (!editor || !monaco || !model) return;
    monaco.editor.setModelMarkers(
      model,
      'baml',
      diagnostics.map((d) => ({
        endColumn: d.range.end.character + 1,
        endLineNumber: d.range.end.line + 1,
        message: d.message,
        severity:
          kind(d.severity) === 'warning'
            ? monaco.MarkerSeverity.Warning
            : kind(d.severity) === 'info'
              ? monaco.MarkerSeverity.Info
              : monaco.MarkerSeverity.Error,
        startColumn: d.range.start.character + 1,
        startLineNumber: d.range.start.line + 1,
      })),
    );
    messages.current ??= editor.createDecorationsCollection();
    messages.current.set(
      diagnostics.map((d) => {
        const line = Math.min(d.range.start.line + 1, model.getLineCount());
        const k = kind(d.severity);
        // The message after the line is cut short. The hover has all of it.
        const text =
          d.message.length > 100 ? `${d.message.slice(0, 100)}…` : d.message;
        return {
          options: {
            after: {
              content: `    ${text}`,
              inlineClassName: `pg-msg pg-msg-${k}`,
            },
            className: `pg-line-${k}`,
            isWholeLine: true,
          },
          range: new monaco.Range(line, 1, line, model.getLineMaxColumn(line)),
        };
      }),
    );
    // The project was rechecked, so hints that were empty before its types were known can fill in.
    refreshInlayHints?.();
    refreshLenses?.();
  }, []);

  const wanted = useRef(awake);
  /** Starts the editor's runtime on its code as it is now. */
  const wake = useCallback(() => {
    const model = editorRef.current?.getModel();
    if (!model || handle.current) return;
    const cell = createCell(latest.current);
    handle.current = cell;
    cell.onDiagnostics(showDiagnostics);
    byModel.set(model, { handle: cell, output: output.current });
  }, [showDiagnostics]);
  /** Ends the editor's runtime. The editor, its code, and its output stay. */
  const sleep = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    handle.current?.dispose();
    handle.current = null;
    const model = editorRef.current?.getModel();
    if (model) byModel.set(model, { handle: null, output: output.current });
  }, []);

  const beforeMount: BeforeMount = useCallback((monaco) => {
    registerBaml(monaco);
  }, []);

  const onMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      monaco.editor.setTheme(BAML_THEME);
      registerProviders(monaco);
      if (wanted.current) wake();
      const fit = () =>
        setHeight(Math.min(maxHeight, Math.max(44, editor.getContentHeight())));
      editor.onDidContentSizeChange(fit);
      fit();
    },
    [maxHeight, wake],
  );

  // The runtime follows `awake`: started when the editor comes near the window, ended when it
  // has gone well past.
  useEffect(() => {
    wanted.current = awake;
    if (awake) wake();
    else sleep();
  }, [awake, wake, sleep]);

  const onChange = useCallback((value?: string) => {
    latest.current = value ?? '';
    if (timer.current) clearTimeout(timer.current);
    // Edits are sent 200ms after the last keystroke, so the compiler sees a burst of typing once.
    timer.current = setTimeout(() => {
      timer.current = null;
      handle.current?.update(latest.current);
    }, 200);
  }, []);

  /** On unmount, the cell's worker ends. */
  const root = useCallback((node: HTMLDivElement | null) => {
    if (!node) return undefined;
    return () => {
      if (timer.current) clearTimeout(timer.current);
      const model = editorRef.current?.getModel();
      if (model) byModel.delete(model);
      handle.current?.dispose();
      handle.current = null;
      editorRef.current = null;
    };
  }, []);

  return (
    <div
      className="overflow-hidden rounded-xl border border-rule bg-[#121212]"
      ref={root}
    >
      {(filename || runnable) && (
        <div className="flex h-10 items-center justify-between gap-3 border-b border-rule pr-2 pl-4">
          <span className="truncate font-mono text-[12px] text-body">
            {filename}
          </span>
          {runnable && (
            <button
              className="focus-ring cursor-pointer rounded-md px-2.5 py-1 font-mono text-[12px] text-clover transition-colors hover:bg-clover/10 disabled:cursor-default disabled:text-muted disabled:hover:bg-transparent"
              disabled={running}
              onClick={run}
              type="button"
            >
              {running ? 'Running…' : '▶ Run'}
            </button>
          )}
        </div>
      )}
      <div style={{ height }}>
        <Editor
          beforeMount={beforeMount}
          defaultLanguage="baml"
          defaultValue={code}
          height="100%"
          loading={null}
          onChange={onChange}
          onMount={onMount}
          options={{
            automaticLayout: true,
            codeLens: runnable,
            // Hovers and completions are drawn over the page, not clipped by the frame.
            fixedOverflowWidgets: true,
            fontFamily: MONO_FONT,
            fontSize: 13,
            guides: { indentation: false },
            hideCursorInOverviewRuler: true,
            hover: { above: false },
            inlayHints: { enabled: 'on' },
            lineDecorationsWidth: 16,
            // Twenty pixels a line, which is what the block is drawn at before the editor loads
            // (./live-code.tsx).
            lineHeight: 20,
            lineNumbers: 'off',
            minimap: { enabled: false },
            overviewRulerLanes: 0,
            padding: { bottom: 12, top: 12 },
            renderLineHighlight: 'none',
            scrollBeyondLastLine: false,
            scrollbar: {
              // The page keeps scrolling when the pointer rests on the editor.
              alwaysConsumeMouseWheel: false,
              horizontalSliderSize: 6,
              verticalSliderSize: 6,
            },
            tabSize: 4,
            wordWrap: 'on',
          }}
          theme={BAML_THEME}
        />
      </div>
      {lines.length > 0 && (
        <ul
          aria-live="polite"
          className="border-t border-rule px-4 py-3 font-mono text-[12.5px] leading-[1.6]"
        >
          {lines.map((l) => (
            <li className="flex gap-2.5" key={l.id}>
              <span aria-hidden className={`shrink-0 ${MARK[l.status].tone}`}>
                {MARK[l.status].sign}
              </span>
              <span className="min-w-0">
                <span className="text-ink-2">{l.name}</span>
                {l.status === 'running' && (
                  <span className="text-muted"> running</span>
                )}
                {l.detail && <span className="text-body"> {l.detail}</span>}
                {l.tests?.map((t) => (
                  <span className="block pl-1" key={t.name}>
                    <span
                      aria-hidden
                      className={MARK[t.passed ? 'pass' : 'fail'].tone}
                    >
                      {MARK[t.passed ? 'pass' : 'fail'].sign}
                    </span>{' '}
                    <span className="text-body">{t.name}</span>
                  </span>
                ))}
                {l.value && (
                  <pre className="mt-1 mb-1.5 overflow-x-auto whitespace-pre text-head">
                    {l.value}
                  </pre>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
