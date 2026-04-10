'use client';
import { BAML } from '@boundaryml/baml-lezer';
import { linter } from '@codemirror/lint';
import { vscodeLightInit } from '@uiw/codemirror-theme-vscode';
import CodeMirror, {
  Compartment,
  EditorView,
  type Extension,
  type ReactCodeMirrorRef,
} from '@uiw/react-codemirror';
import { inlineCopilot } from 'codemirror-copilot';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { hyperLink } from '@uiw/codemirror-extensions-hyper-link';
import { langs } from '@uiw/codemirror-extensions-langs';
import { useStore } from 'jotai';
import { CodeMirrorDiagnosticsAtom } from '../atoms';
import { type ICodeBlock } from '../types';

import { autocompletion } from '@codemirror/autocomplete';
import { javascript } from '@codemirror/lang-javascript';
import {
  tsAutocomplete,
  tsFacet,
  tsHover,
  tsLinter,
  tsSync,
} from '@valtown/codemirror-ts';

import {
  createDefaultMapFromCDN,
  createSystem,
  createVirtualTypeScriptEnvironment,
} from '@typescript/vfs';
import ts from 'typescript';

const extensionMap = {
  js: [langs.javascript()],
  jsx: [langs.jsx()],
  py: [langs.python()],
  python: [langs.python()],
  json: [langs.json()],
  baml: [BAML()],
};

export interface GeneratedFile {
  path: string;
  content: string;
}

export const CodeMirrorViewer = ({
  lang,
  fileContent,
  generatedFiles,
  shouldScrollDown,
  isReadOnly,
  onContentChange,
  onAutocompleteTrigger,
}: {
  lang: ICodeBlock['language'];
  fileContent: ICodeBlock;
  generatedFiles?: GeneratedFile[];
  shouldScrollDown: boolean;
  isReadOnly?: boolean;
  onContentChange: (content: string) => void;
  onAutocompleteTrigger?: (content: string) => Promise<string>;
}) => {
  const ref = useRef<ReactCodeMirrorRef>({});
  const store = useStore();

  const makeLinter = useCallback(() => {
    if (lang === 'baml') {
      return linter(
        () => {
          try {
            const diags = store.get(CodeMirrorDiagnosticsAtom);
            return diags.map((d) => {
              return {
                from: d.from,
                // seems to be off by one after adding the copilot extension?
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
    } else if (lang === 'python') {
      // use ruff wasm here
      // return linter(
      //   (view) => {
      //     const doc = view.state.doc;
      //     const res = ruffW;
      //     return diags;
      //   },
      //   { delay: 200 }
      // );
    }
    return [];
  }, [store, lang]);

  const compartment = useMemo(() => new Compartment(), []);

  const [extensions, setExtensions] = useState<Extension[]>([]);

  useEffect(() => {
    // const interval = setInterval(() => {
    if (ref.current?.view?.contentDOM) {
      const line = ref.current.view.state.doc.lineAt(
        ref.current.view.state.doc.length,
      );
      if (line) {
        if (shouldScrollDown) {
          ref.current.view?.dispatch({
            selection: { anchor: line.from, head: undefined },
            scrollIntoView: true,
          });
        }
      }

      // // Scroll to the bottom of the container
      // containerRef.current.contentDOM.scrollIntoView({
      //   behavior: "smooth",
      // });
    }
    // }, 1000); // Adjust the interval time (in milliseconds) as needed

    // return () => clearInterval(interval); // Clean up the interval on component unmount
  }, [fileContent, ref, shouldScrollDown]);

  useEffect(() => {
    async function initializeExtensions() {
      try {
        if (lang === 'typescript') {
          console.log('initializing ts extensions');
          const fsMap = await createDefaultMapFromCDN(
            { target: ts.ScriptTarget.ES2022 },
            '5.6.3',
            true,
            ts,
          );
          const system = createSystem(fsMap);
          const compilerOpts = {
            lib: ['es2022', 'dom'],
          };
          const baml_dir = '/baml_client';
          let generated_files_paths: string[] = [];
          // I dont think we need to update fsmap..
          if (generatedFiles) {
            generated_files_paths = generatedFiles.map(
              (f) => baml_dir + '/' + f.path,
            );
            generatedFiles.forEach((f) => {
              fsMap.set(baml_dir + '/' + f.path, f.content);
            });
          }
          const env = createVirtualTypeScriptEnvironment(
            system,
            generated_files_paths,
            ts,
            compilerOpts,
          );
          for (const f of generatedFiles ?? []) {
            env.createFile(baml_dir + '/' + f.path, f.content);
          }

          const tsExtensions = [
            langs.typescript(),
            javascript({ typescript: true }),
            tsFacet.of({ env, path: 'index.ts' }),
            tsSync(),
            tsLinter(),
            tsHover(),
            autocompletion({
              override: [tsAutocomplete()],
            }),
          ];

          setExtensions([...tsExtensions, EditorView.lineWrapping, hyperLink]);
          return;
        }

        setExtensions([
          ...(extensionMap[lang as keyof typeof extensionMap] || []),
          EditorView.lineWrapping,
          lang === 'baml' ? compartment.of(makeLinter()) : [],
          hyperLink,
        ]);
      } catch (e) {
        console.error('Error initializing extensions', e);
        setExtensions([]);
      }
    }

    void initializeExtensions();
  }, [
    JSON.stringify(generatedFiles?.map((f) => f.content)),
    compartment,
    lang,
    makeLinter,
  ]);

  const inlineCopilotExtension = useMemo(
    () => [
      inlineCopilot(async (prefix: string, suffix: string) => {
        if (isReadOnly) {
          return '';
        }
        const res = await fetch('/api/completion', {
          method: 'POST',
          body: JSON.stringify({ prefix, suffix, language: lang }),
        });
        const { prediction } = await res.json();
        return prediction;
      }, 500),
    ],
    [lang, isReadOnly],
  );

  const theme = vscodeLightInit({
    settings: {
      fontSize: '11px',
      // this must be a transparent color or selection will be invisible
      lineHighlight: '#c7c7c76e',
      gutterBackground: '#fff',
      gutterForeground: '#808080',
      gutterActiveForeground: '#808080',
      gutterBorder: '#fff',
    },
  });

  useEffect(() => {
    onContentChange?.(fileContent.code);
  }, [fileContent.code]);

  useEffect(() => {
    if (lang !== 'baml') {
      return;
    }
    if (ref.current?.view) {
      const view = ref.current.view;
      view.dispatch({
        effects: compartment.reconfigure([makeLinter()]),
      });
    }
  }, [fileContent.code, lang, makeLinter, compartment]);

  const handleReset = () => {
    // setActualFileContent(file_content);
  };

  const handleSave = () => {
    console.log('Saving changes...');
  };

  return (
    <div className="h-fit w-full">
      <div
        className="h-full w-full"
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === 's') {
            e.preventDefault();
          }
        }}
      >
        <CodeMirror
          ref={ref}
          key={lang}
          id={lang}
          value={fileContent.code}
          onChange={(value) => {
            onContentChange?.(value);
          }}
          readOnly={false}
          extensions={[...extensions, ...inlineCopilotExtension]}
          theme={theme}
          className="border-none text-xs"
          height="100%"
          width="100%"
          style={{ width: '100%', height: '100%' }}
        />
        {/* {modified && (
          <>
            <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-40 bg-gradient-to-t from-white to-transparent" />
            <div className="absolute bottom-10 left-0 right-0 flex justify-center">
              <div className="flex items-center gap-2 rounded-full bg-white px-4 py-2 shadow-lg">
                <AlertTriangle className="h-4 w-4 text-yellow-500" />
                <span className="text-sm font-medium">Unsaved Changes</span>
                <div className="ml-2 flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleReset}
                    className="h-8 px-4"
                  >
                    Reset
                  </Button>
                  <Button
                    size="sm"
                    onClick={handleSave}
                    className="h-8 bg-black px-4 text-white hover:bg-black/90"
                  >
                    Save
                  </Button>
                </div>
              </div>
            </div>
          </>
        )} */}
      </div>
    </div>
  );
};

CodeMirrorViewer.displayName = 'CodeMirrorViewer';
