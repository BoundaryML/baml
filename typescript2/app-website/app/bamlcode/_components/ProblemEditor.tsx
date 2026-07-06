'use client';

import Editor, { type BeforeMount, type OnMount } from '@monaco-editor/react';
import { useCallback, useEffect, useRef } from 'react';
import { registerBaml } from '@/app/learn2/_lib/baml-monarch';
import { useCodeTheme } from '@/app/learn2/_lib/code-theme';
import type { LspDiagnostic, SolveRuntime } from '../_lib/runtime';

type EditorInstance = Parameters<OnMount>[0];
type MonacoNs = Parameters<OnMount>[1];

function markerSeverity(monaco: MonacoNs, s?: number) {
  if (s === 2) return monaco.MarkerSeverity.Warning;
  if (s === 3 || s === 4) return monaco.MarkerSeverity.Info;
  return monaco.MarkerSeverity.Error;
}

export interface ProblemEditorProps {
  runtime: SolveRuntime;
  initialCode: string;
  /**
   * Programmatic code injection: whenever `seq` changes, the editor is set to
   * `code` and the runtime re-synced. Used for "reset" and "load reference".
   */
  inject?: { code: string; seq: number };
  height?: number | string;
}

/**
 * Monaco editor bound to a {@link SolveRuntime}: keystrokes flow to the runtime
 * (recompile + diagnostics) and diagnostics flow back as inline markers.
 */
export function ProblemEditor({
  runtime,
  initialCode,
  inject,
  height = '100%',
}: ProblemEditorProps) {
  const codeTheme = useCodeTheme();
  const themeRef = useRef(codeTheme.monaco);
  themeRef.current = codeTheme.monaco;

  const editorRef = useRef<EditorInstance | null>(null);
  const monacoRef = useRef<MonacoNs | null>(null);
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
        endColumn: d.range.end.character + 1,
        endLineNumber: d.range.end.line + 1,
        message: d.message,
        severity: markerSeverity(monaco, d.severity),
        startColumn: d.range.start.character + 1,
        startLineNumber: d.range.start.line + 1,
      })),
    );
  }, []);

  const beforeMount: BeforeMount = useCallback((monaco) => {
    registerBaml(monaco);
  }, []);

  const onMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;
      monaco.editor.setTheme(themeRef.current);
      runtime.onDiagnostics(applyDiagnostics);
    },
    [runtime, applyDiagnostics],
  );

  const onChange = useCallback(
    (value?: string) => {
      const next = value ?? '';
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => runtime.updateSolution(next), 200);
    },
    [runtime],
  );

  // Injection: set the editor to the requested code and re-sync the runtime.
  const injectSeq = inject?.seq ?? 0;
  const lastInjectRef = useRef(injectSeq);
  useEffect(() => {
    if (injectSeq === lastInjectRef.current) return;
    lastInjectRef.current = injectSeq;
    const editor = editorRef.current;
    if (!editor || !inject) return;
    editor.setValue(inject.code);
    runtime.updateSolution(inject.code);
  }, [injectSeq, inject, runtime]);

  // Cancel any pending debounced edit if the editor unmounts (route change)
  // so the callback can't fire against a disposed model.
  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  return (
    <Editor
      beforeMount={beforeMount}
      defaultLanguage="baml"
      defaultValue={initialCode}
      height={height}
      onChange={onChange}
      onMount={onMount}
      options={{
        automaticLayout: true,
        fontFamily: 'var(--font-geist-mono), ui-monospace, monospace',
        fontSize: 13,
        guides: { indentation: false },
        lineNumbers: 'on',
        minimap: { enabled: false },
        overviewRulerLanes: 0,
        padding: { bottom: 12, top: 12 },
        renderLineHighlight: 'line',
        scrollBeyondLastLine: false,
        tabSize: 2,
      }}
      theme={codeTheme.monaco}
    />
  );
}
