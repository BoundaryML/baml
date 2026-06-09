'use client';

import Editor, { type BeforeMount, type OnMount } from '@monaco-editor/react';
import {
  ExecutionPanel,
  type RuntimePort,
  WorkerRuntimePort,
} from '@b/pkg-playground';
import { useCallback, useRef, useState } from 'react';
import { getBamlWorker } from '@/playground/spawnBamlWorker';
import { registerBaml } from '../_lib/baml-monarch';

interface LivePlaygroundProps {
  initialCode: string;
  /** Function to preselect in the panel (e.g. the pipeline entrypoint). */
  initialFunction?: string;
  /** Panel tab to open on (default 'graph' — the viz is the point). */
  initialTab?: 'run' | 'graph' | 'prompt' | 'curl';
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
  initialFunction,
  initialTab = 'graph',
}: LivePlaygroundProps) {
  const [port, setPort] = useState<RuntimePort | null>(null);
  const [version, setVersion] = useState(0);
  const [failed, setFailed] = useState(false);
  const portRef = useRef<RuntimePort | null>(null);
  const codeRef = useRef(initialCode);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cursorDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoNs | null>(null);
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
          startLineNumber: d.range.start.line + 1,
          startColumn: d.range.start.character + 1,
          endLineNumber: d.range.end.line + 1,
          endColumn: d.range.end.character + 1,
          message: d.message,
          severity: sev,
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
          range: new monaco.Range(line, 1, line, endCol),
          options: {
            after: {
              content: `    ${inline}`,
              inlineClassName: `l2-el-msg l2-el-msg-${kind}`,
              inlineClassNameAffectsLetterSpacing: true,
            },
          },
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
      type: 'cursorPosition',
      file: 'baml_src/main.baml',
      line: pos.lineNumber - 1,
      column: pos.column - 1,
    });
  }, []);

  const beforeMount: BeforeMount = useCallback((monaco) => {
    registerBaml(monaco);
  }, []);

  const onMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      monaco.editor.setTheme('baml-paper');

      // Forward caret moves (debounced) so clicking in the code selects the
      // matching function/test in the playground panel.
      editor.onDidChangeCursorPosition(() => {
        if (cursorDebounceRef.current) clearTimeout(cursorDebounceRef.current);
        cursorDebounceRef.current = setTimeout(postCursor, 50);
      });

      getBamlWorker(codeRef.current)
        .then((worker) => {
          worker.addEventListener('message', (e: MessageEvent) => {
            if (e.data?.type === 'lspDiagnostics') {
              applyDiagnostics(e.data.params as LspPublish);
            }
          });

          const p = new WorkerRuntimePort(worker);
          portRef.current = p;
          setPort(p);
          setVersion((v) => v + 1);
          // The worker emits its project state during init — before the panel
          // is listening — so the function/test list can start empty. Nudge a
          // re-eval once mounted so functions + diagnostics show without an edit.
          setTimeout(() => {
            p.postMessage({
              type: 'filesChanged',
              files: { 'baml_src/main.baml': codeRef.current },
            });
            // Seed the panel selection from the initial caret (best-effort —
            // the project may still be collecting; the first click corrects it).
            postCursor();
          }, 200);
        })
        .catch(() => setFailed(true));
    },
    [applyDiagnostics, postCursor],
  );

  const onChange = useCallback((value?: string) => {
    const next = value ?? '';
    codeRef.current = next;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    // 200ms debounce: rapid keystrokes must coalesce or the runtime panics.
    debounceRef.current = setTimeout(() => {
      portRef.current?.postMessage({
        type: 'filesChanged',
        files: { 'baml_src/main.baml': codeRef.current },
      });
    }, 200);
  }, []);

  return (
    <div className="baml-playground-root l2-live">
      <div className="l2-live-editor">
        <Editor
          defaultLanguage="baml"
          defaultValue={initialCode}
          theme="baml-paper"
          beforeMount={beforeMount}
          onMount={onMount}
          onChange={onChange}
          height="100%"
          options={{
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
              // Don't trap page scroll when the cursor rests on the editor.
              alwaysConsumeMouseWheel: false,
            },
            padding: { top: 12, bottom: 12 },
            tabSize: 2,
            wordWrap: 'on',
            automaticLayout: true,
            guides: { indentation: false },
            hover: { above: false },
          }}
        />
      </div>
      <div className="l2-live-panel">
        {failed ? (
          <div className="l2-live-loading">runtime failed to start</div>
        ) : port ? (
          <ExecutionPanel
            port={port}
            connectionVersion={version}
            initialArgsJson="{}"
            initialTab={initialTab}
            initialFunctionName={initialFunction}
          />
        ) : (
          <div className="l2-live-loading">starting runtime…</div>
        )}
      </div>
    </div>
  );
}
