'use client';
import { BAML } from '@baml/codemirror-lang-baml';
import { linter } from '@codemirror/lint';
import { RangeSet, StateEffect, StateField } from '@codemirror/state';
import { Decoration } from '@codemirror/view';
import { tags as t } from '@lezer/highlight';
import { vscodeDarkInit, vscodeLightInit } from '@uiw/codemirror-theme-vscode';
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
import { useAtomValue, useSetAtom, useStore } from 'jotai';
import type { ICodeBlock } from '../types';
import { CodeMirrorDiagnosticsAtom } from './atoms';

import { autocompletion } from '@codemirror/autocomplete';
import { javascript } from '@codemirror/lang-javascript';
import {
  tsAutocomplete,
  tsFacet,
  tsHover,
  tsLinter,
  tsSync,
} from '@valtown/codemirror-ts';

// import {
//   createDefaultMapFromCDN,
//   createSystem,
//   createVirtualTypeScriptEnvironment,
// } from '@typescript/vfs';
import { useTheme } from 'next-themes';
import ts from 'typescript';
import { flashRangesAtom, updateCursorAtom } from '../playground-panel/atoms';

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

const addFlashingEffect = StateEffect.define<{ from: number; to: number }[]>();
const clearFlashingEffect = StateEffect.define<void>();
const flashingMark = Decoration.mark({
  attributes: {
    style: `
      background-color: transparent;
      animation: flashGlow 800ms cubic-bezier(0.4, 0, 0.2, 1) 1;
      will-change: text-shadow;
    `,
  },
});

// Add the animation keyframes to the document
if (typeof document !== 'undefined') {
  const style = document.createElement('style');
  style.textContent = `
    @keyframes flashGlow {
      0% { text-shadow: 0 0 0 rgba(255, 235, 59, 0); }
      50% { text-shadow: 0 0 3px rgba(255, 235, 59, 0.85), 0 0 8px rgba(255, 235, 59, 0.75), 0 0 14px rgba(255, 235, 59, 0.6); }
      100% { text-shadow: 0 0 0 rgba(255, 235, 59, 0); }
    }
  `;
  document.head.appendChild(style);
}

// Create a StateField for the highlights
const createFlashingField = () => {
  return StateField.define({
    create() {
      return RangeSet.empty;
    },
    update(highlights, tr) {
      // Map the decorations through document changes
      highlights = highlights.map(tr.changes);

      // Apply effects
      for (const effect of tr.effects) {
        if (effect.is(addFlashingEffect)) {
          // Create new highlight decorations
          const decorations = effect.value.map((range) =>
            flashingMark.range(range.from, range.to),
          );

          // Add them to the set
          highlights = highlights.update({
            add: decorations,
            sort: true,
          });
        } else if (effect.is(clearFlashingEffect)) {
          // Clear all decorations
          highlights = RangeSet.empty;
        }
      }

      return highlights;
    },
    provide: (field) => EditorView.decorations.from(field),
  });
};

export const CodeMirrorViewer = ({
  lang,
  fileContent,
  generatedFiles,
  shouldScrollDown,
  isReadOnly,
  hideLineNumbers,
  onContentChange,
  onAutocompleteTrigger,
}: {
  lang: ICodeBlock['language'];
  fileContent: ICodeBlock;
  generatedFiles?: GeneratedFile[];
  shouldScrollDown: boolean;
  isReadOnly?: boolean;
  hideLineNumbers?: boolean;
  onContentChange: (content: string) => void;
  onAutocompleteTrigger?: (content: string) => Promise<string>;
}) => {
  // const [files, setFiles] = useAtom(filesAtom)

  const ref = useRef<ReactCodeMirrorRef>({});
  const store = useStore();
  const flashRanges = useAtomValue(flashRangesAtom);
  const diagnostics = useAtomValue(CodeMirrorDiagnosticsAtom);

  useEffect(() => {
    console.log('flashRanges updated: ', flashRanges);
    if (!ref.current.view) return;
    const view = ref.current.view;
    // Only show/act on ranges that correspond to the currently open file
    const relevantRanges = flashRanges.filter(
      (range) => range.filePath === fileContent.id,
    );
    const convertedRanges = relevantRanges.map((range) => ({
      from: view.state.doc.line(range.startLine + 1).from + range.startCol,
      to: view.state.doc.line(range.endLine + 1).from + range.endCol,
    }));
    // Update flashing decorations
    view.dispatch({
      effects: [
        clearFlashingEffect.of(),
        addFlashingEffect.of(convertedRanges),
      ],
    });
    // Select and center the first range in the viewport
    const first = convertedRanges[0];
    if (first !== undefined) {
      view.dispatch({
        selection: { anchor: first.from, head: first.to },
        effects: [EditorView.scrollIntoView(first.from, { y: 'center' })],
      });
    }

  }, [flashRanges, fileContent.id]);


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
        { delay: 300 },
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
  }, [store, lang, CodeMirrorDiagnosticsAtom]);

  const compartment = useMemo(() => new Compartment(), []);

  const [extensions, setExtensions] = useState<Extension[]>([]);

  // useEffect(() => {
  //   // const interval = setInterval(() => {
  //   if (ref.current?.view?.contentDOM) {
  //     const line = ref.current.view.state.doc.lineAt(ref.current.view.state.doc.length)
  //     if (line) {
  //       if (shouldScrollDown) {
  //         ref.current.view?.dispatch({
  //           selection: { anchor: line.from, head: undefined },
  //           scrollIntoView: true,
  //         })
  //       }
  //     }
  //     // // Scroll to the bottom of the container
  //     // containerRef.current.contentDOM.scrollIntoView({
  //     //   behavior: "smooth",
  //     // });
  //   }
  //   // }, 1000); // Adjust the interval time (in milliseconds) as needed

  //   // return () => clearInterval(interval); // Clean up the interval on component unmount
  // }, [fileContent, ref, shouldScrollDown])

  const setUpdateCursor = useSetAtom(updateCursorAtom);

  useEffect(() => {
    async function initializeExtensions() {
      try {
        if (lang === 'typescript') {
          // console.log('initializing ts extensions');
          // const fsMap = await createDefaultMapFromCDN(
          //   { target: ts.ScriptTarget.ES2022 },
          //   '5.6.3',
          //   true,
          //   ts,
          // );
          // const system = createSystem(fsMap);
          // const compilerOpts = {
          //   lib: ['es2022', 'dom'],
          // };
          // const baml_dir = '/baml_client';
          // let generated_files_paths: string[] = [];
          // // I dont think we need to update fsmap..
          // if (generatedFiles) {
          //   generated_files_paths = generatedFiles.map(
          //     (f) => baml_dir + '/' + f.path,
          //   );
          //   generatedFiles.forEach((f) => {
          //     fsMap.set(baml_dir + '/' + f.path, f.content);
          //   });
          // }
          // const env = createVirtualTypeScriptEnvironment(
          //   system,
          //   generated_files_paths,
          //   ts,
          //   compilerOpts,
          // );
          // for (const f of generatedFiles ?? []) {
          //   env.createFile(baml_dir + '/' + f.path, f.content);
          // }

          // const tsExtensions = [
          //   langs.typescript(),
          //   javascript({ typescript: true }),
          //   tsFacet.of({ env, path: 'index.ts' }),
          //   tsSync(),
          //   tsLinter(),
          //   tsHover(),
          //   autocompletion({
          //     override: [tsAutocomplete()],
          //   }),
          // ];

          setExtensions([
            // ...tsExtensions,
            EditorView.lineWrapping,
            hyperLink,
            createFlashingField(),
          ]);
          return;
        }

        setExtensions([
          ...(extensionMap[lang as keyof typeof extensionMap] || []),
          EditorView.lineWrapping,
          lang === 'baml' ? compartment.of(makeLinter()) : [],
          hyperLink,
          createFlashingField(),
        ]);
      } catch (e) {
        console.error('Error initializing extensions', e);
        setExtensions([]);
      }
    }

    void initializeExtensions();
  }, [
    JSON.stringify(generatedFiles?.map((f) => f.path)),
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

  const { theme } = useTheme();

  const editorTheme =
    theme === 'dark'
      ? vscodeDarkInit({
          styles: [
            {
              tag: [t.variableName],
              color: '#dcdcaa',
            },
            {
              tag: [t.brace],
              color: '#569cd6',
            },
            {
              tag: [t.variableName, t.propertyName],
              color: '#d4d4d4',
            },
            {
              tag: [t.attributeName],
              color: '#c586c0',
            },
          ],
          settings: {
            fontSize: '11px',
            // this must be a transparent color or selection will be invisible
            lineHighlight: '#a1a1a730',
            gutterBackground: 'transparent',
            gutterForeground: '#808080',
            gutterActiveForeground: '#808080',
            gutterBorder: 'transparent',
          },
        })
      : vscodeLightInit({
          styles: [
            {
              tag: [t.attributeName],
              color: '#ca8a04',
            },
          ],
          settings: {
            fontSize: '11px',
            // this must be a transparent color or selection will be invisible
            lineHighlight: '#c7c7c730',
            gutterBackground: '#fff',
            gutterForeground: '#808080',
            gutterActiveForeground: '#808080',
            gutterBorder: '#fff',
          },
        });

  useEffect(() => {
    onContentChange?.(fileContent.code);
  }, [fileContent.id]);

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
  }, [fileContent.code, lang, makeLinter, compartment, diagnostics]);

  const handleReset = () => {
    // setActualFileContent(file_content);
  };

  const handleSave = () => {
    console.log('Saving changes...');
  };

  return (
    <div className="w-full h-fit">
      <div
        className="pb-8 w-full h-full"
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
          basicSetup={{
            lineNumbers: !hideLineNumbers,
            foldGutter: hideLineNumbers ? false : true,
          }}
          onStatistics={(data) => {
            const pos = data.selectionAsSingle.from;
            const line = data.line.number;
            // Calculate column by finding the difference between cursor position and line start
            const column = pos - data.line.from + 1;

            setUpdateCursor({
              fileName: fileContent.id,
              fileText: ref.current?.view?.state.doc.toString() || '',
              line,
              column,
            });
          }}
          value={fileContent.code}
          onChange={(value) => {
            onContentChange?.(value);
          }}
          readOnly={false}
          extensions={[...extensions]}
          theme={editorTheme}
          className="text-xs border-none"
          height="100%"
          width="100%"
          style={{ width: '100%', height: '100%' }}
        />
        {/* {modified && (
          <>
            <div className="absolute right-0 bottom-0 left-0 h-40 bg-linear-to-t from-white to-transparent pointer-events-none" />
            <div className="flex absolute right-0 left-0 bottom-10 justify-center">
              <div className="flex gap-2 items-center px-4 py-2 bg-white rounded-full shadow-lg">
                <AlertTriangle className="w-4 h-4 text-yellow-500" />
                <span className="text-sm font-medium">Unsaved Changes</span>
                <div className="flex gap-2 items-center ml-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleReset}
                    className="px-4 h-8"
                  >
                    Reset
                  </Button>
                  <Button
                    size="sm"
                    onClick={handleSave}
                    className="px-4 h-8 text-white bg-black hover:bg-black/90"
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
