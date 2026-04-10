'use client';

import {
  currentCodeBlock,
  runtimeAtom,
  diagnosticsAtom,
  CodeMirrorDiagnosticsAtom,
  lessonsAtom,
  visibleFileAtom,
  safeCurrentIndicesAtom,
} from '../lib/learn-atoms';
import { lessonAndPageIdAtom } from '../lib/learn-atoms';
import CodeMirror, {
  Compartment,
  EditorView,
  type Extension,
  type ReactCodeMirrorRef,
  Transaction,
  ChangeDesc,
} from '@uiw/react-codemirror';
import { linter } from '@codemirror/lint';
import { useAtom, useAtomValue, useStore } from 'jotai';
import { vscodeLightInit } from '@uiw/codemirror-theme-vscode';
import { BAML } from '@boundaryml/baml-lezer';
import { langs } from '@uiw/codemirror-extensions-langs';
import { javascript } from '@codemirror/lang-javascript';
import { python } from '@codemirror/lang-python';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

export const CodeEditor = () => {
  const [runtime, setRuntime] = useAtom(runtimeAtom);
  const [diagnostics, setDiagnostics] = useAtom(diagnosticsAtom);
  const store = useStore();
  const fileContent = useAtomValue(visibleFileAtom);
  const [extensions, setExtensions] = useState<Extension[]>([]);
  const [lessons, setLessons] = useAtom(lessonsAtom);
  const { lessonIndex, pageIndex, tabId, lesson, page } = useAtomValue(
    safeCurrentIndicesAtom,
  );
  const compartment = useMemo(() => new Compartment(), []);
  // console.log(diagnostics)

  const ref = useRef<ReactCodeMirrorRef>({});

  // Use VS Code theme like in code-mirror-viewer
  const theme = vscodeLightInit({
    settings: {
      fontSize: '11px',
      lineHighlight: '#c7c7c76e',
      gutterBackground: '#fff',
      gutterForeground: '#808080',
      gutterActiveForeground: '#808080',
      gutterBorder: '#fff',
    },
  });

  const makeLinter = useCallback(() => {
    if (fileContent.language === 'baml') {
      return linter(
        () => {
          try {
            const diags = store.get(CodeMirrorDiagnosticsAtom);
            return diags.map((d) => {
              return {
                from: d.from,
                to: d.to - 1,
                severity: d.severity,
                message: d.message,
              };
            });
          } catch (e) {
            console.error('Error getting diagnostics', e);
            return [];
          }
        },
        { delay: 200 },
      );
    }
    return [];
  }, [store, fileContent.language]);

  // Get language extensions based on file type
  const getLanguageExtension = (language: string): Extension[] => {
    switch (language) {
      case 'baml':
        return [BAML()];
      case 'python':
        return [python()];
      case 'typescript':
        return [javascript({ typescript: true })];
      case 'javascript':
        return [javascript()];
      default:
        return [];
    }
  };

  useEffect(() => {
    async function initializeExtensions() {
      try {
        const langExtensions = getLanguageExtension(fileContent.language);
        setExtensions([
          ...langExtensions,
          EditorView.lineWrapping,
          fileContent.language === 'baml' ? compartment.of(makeLinter()) : [],
        ]);
      } catch (e) {
        console.error('Error initializing extensions', e);
        setExtensions([]);
      }
    }
    initializeExtensions();
  }, [makeLinter, compartment, fileContent.language]);

  function handleCodeChange(newCode: string) {}

  // Update linter when code changes for BAML files
  useEffect(() => {
    if (fileContent.language !== 'baml') {
      return;
    }
    if (ref.current?.view) {
      const view = ref.current.view;
      view.dispatch({
        effects: compartment.reconfigure([makeLinter()]),
      });
    }
  }, [fileContent.code, fileContent.language, makeLinter, compartment]);

  return (
    <div className="w-full h-full">
      <CodeMirror
        ref={ref}
        key={fileContent.language}
        id={fileContent.id}
        basicSetup={{
          lineNumbers: true,
          foldGutter: true,
        }}
        extensions={extensions}
        theme={theme}
        className="border-none text-xs"
        height="100%"
        width="100%"
        value={fileContent.code}
        onChange={(value) => {
          const newLessons = [...lessons];
          const lesson = newLessons[lessonIndex];
          const page = lesson?.pages?.[pageIndex];
          if (page && page.fileContents) {
            page.fileContents[tabId as keyof typeof page.fileContents] = value;
            setLessons(newLessons);
          }
        }}
      />
    </div>
  );
};
