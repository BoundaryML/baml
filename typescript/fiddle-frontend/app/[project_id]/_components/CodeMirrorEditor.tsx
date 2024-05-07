'use client'
import { EditorFile } from '@/app/actions'
import { BAML_DIR } from '@/lib/constants'
import { BAMLProject } from '@/lib/exampleProjects'
import { BAML, theme } from '@baml/codemirror-lang'
import { ParserDatabase } from '@baml/common'
import { Button } from '@baml/playground-common/components/ui/button'
import { Language, LanguageSupport } from '@codemirror/language'
import { Diagnostic, forceLinting, linter } from '@codemirror/lint'
import { langs } from '@uiw/codemirror-extensions-langs'
import CodeMirror, { Compartment, EditorView, Extension, ReactCodeMirrorRef } from '@uiw/react-codemirror'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import Link from 'next/link'
import { useEffect, useRef } from 'react'
import {
  PROJECT_ROOT,
  activeFileContentAtom,
  activeFileNameAtom,
  currentEditorFilesAtom,
  unsavedChangesAtom,
} from '../_atoms/atoms'
import { atomStore } from '@baml/playground-common/baml_wasm_web/JotaiProvider'
import { projectFamilyAtom, runtimeFamilyAtom } from '@baml/playground-common/baml_wasm_web/baseAtoms'
import { diagnositicsAtom, updateFileAtom } from '@baml/playground-common/baml_wasm_web/EventListener'

type LintResponse = {
  diagnostics: LinterError[]
} & (
  | { ok: false }
  | {
      ok: true
      response: ParserDatabase
    }
)

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
  source_file: string
}

export interface LinterSourceFile {
  path: string
  content: string
}

export interface LinterInput {
  root_path: string
  files: LinterSourceFile[]
  // Function Name -> Test Name
  selected_tests: Record<string, string>
}

let wasmModuleCache: any = null

// async function bamlLinter(_view: any): Promise<Diagnostic[]> {
//   if (!wasmModuleCache) {
//     wasmModuleCache = await import('@gloo-ai/baml-schema-wasm-web')
//   }
//   const lint = wasmModuleCache.lint
//   const currentFiles = atomStore.get(currentEditorFilesAtom) as EditorFile[]
//   const selectedTests = atomStore.get(functionTestCaseAtom) as Record<string, string>
//   const linterInput: LinterInput = {
//     root_path: `${BAML_DIR}`,
//     files: currentFiles.filter((f) => f.path.includes(BAML_DIR)).map((f) => ({ path: f.path, content: f.content })),
//     selected_tests: selectedTests,
//   }

//   const res = lint(JSON.stringify(linterInput))
//   const parsedRes = JSON.parse(res) as LintResponse
//   const BamlDB = new Map<string, any>()
//   BamlDB.set('baml_src', res)

//   if (parsedRes.ok) {
//     atomStore.set(currentParserDbAtom, parsedRes.response)
//   }

//   const allDiagnostics = parsedRes.diagnostics.map((d) => {
//     return {
//       from: d.start,
//       to: d.end,
//       message: d.text,
//       severity: d.is_warning ? 'warning' : ('error' as Diagnostic['severity']),
//       source: d.source_file,
//     }
//   })

//   atomStore.set(fileDiagnostics, allDiagnostics)

//   return allDiagnostics.filter((d) => d.source === atomStore.get(activeFileAtom)?.path)
// }

function makeLinter() {
  return linter(
    (_view) => {
      console.log('running linter')
      const diagnosticErrors = atomStore.get(diagnositicsAtom)
      const currentFile = atomStore.get(activeFileNameAtom)
      if (!currentFile) {
        return []
      }

      console.log('diagnosticErrors', diagnosticErrors)

      return (
        diagnosticErrors
          .filter((err) => err.file_path == currentFile)
          .map((err): Diagnostic => {
            return {
              from: err.start_ch,
              to: err.end_ch,
              message: err.message,
              severity: err.type === 'warning' ? 'warning' : 'error',
              source: 'baml',
            }
          }) ?? []
      )
    },
    { delay: 200 },
  )
}

const comparment = new Compartment()
const extensions: Extension[] = [BAML(), EditorView.lineWrapping, comparment.of(makeLinter())]

const extensionMap = {
  ts: [langs.tsx(), EditorView.lineWrapping],
  py: [langs.python(), EditorView.lineWrapping],
  json: [langs.json(), EditorView.lineWrapping],
  baml: [extensions],
}
const getLanguage = (filePath: string | null): Extension[] => {
  const extension = filePath?.split('.').pop()
  return extensionMap[extension as keyof typeof extensionMap] ?? []
}
export const CodeMirrorEditor = ({ project }: { project: BAMLProject }) => {
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const [activeFile, setActiveFile] = useAtom(activeFileNameAtom)
  const activeFileContent = useAtomValue(activeFileContentAtom)
  const updateFile = useSetAtom(updateFileAtom)

  const ref = useRef<ReactCodeMirrorRef>({})

  // force linting on file changes so playground updates
  useEffect(() => {
    if (ref.current?.view) {
      const view = ref.current.view
      view.dispatch({
        effects: comparment.reconfigure([makeLinter()]),
      })
    }
  }, [editorFiles])

  // ref.current.view.

  const setUnsavedChanges = useSetAtom(unsavedChangesAtom)

  const langExtensions = getLanguage(activeFile)

  return (
    <div className='w-full'>
      <div className='flex px-3 py-1 h-fit gap-x-6 overflow-clip min-h-[20px]'>
        <>
          {editorFiles
            .filter((f) => f.path === activeFile)
            .map((file) => (
              <Button
                variant={'ghost'}
                key={file.path}
                onClick={() => setActiveFile(file.path)}
                disabled={file.path === activeFile}
                className={`${
                  activeFile === file.path
                    ? '  border-b-[2px] border-b-blue-400 bg-background text-blue-500 hover:bg-vscode-selection-background hover:text-blue-500'
                    : 'hover:text-black/80 bg-background text-gray-500 hover:bg-vscode-selection-background hover:text-gray-5=400'
                }  h-[20px] rounded-b-none rounded-tl-lg  border-r-0 px-1 text-xs  font-medium`}
              >
                {file.path.replace(`${BAML_DIR}/`, '')}
              </Button>
            ))}
        </>
      </div>
      <div
        style={{
          height: 'calc(100% - 30px)',
        }}
      >
        <CodeMirror
          ref={ref}
          // key={editorFiles.map((f) => f.path).join('')}
          value={activeFileContent}
          extensions={langExtensions}
          theme={theme}
          className='text-sm'
          height='100%'
          width='100%'
          maxWidth='100%'
          style={{ width: '100%', height: '100%' }}
          onChange={async (val, viewUpdate) => {
            if (activeFile === null) {
              return
            }
            updateFile({
              reason: 'editor',
              root_path: PROJECT_ROOT,
              files: [
                {
                  name: activeFile,
                  content: val,
                },
              ],
            })

            window.history.replaceState(null, '', '/')
            setUnsavedChanges(true)
          }}
        />
      </div>
    </div>
  )
}
