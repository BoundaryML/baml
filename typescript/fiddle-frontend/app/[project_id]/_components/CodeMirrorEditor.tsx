'use client'
import { EditorFile } from '@/app/actions'
import { Button } from '@baml/playground-common/components/ui/button'
import { useAtom, useSetAtom } from 'jotai'
import { usePathname } from 'next/navigation'
import { useState, useEffect, useRef } from 'react'
import {
  activeFileAtom,
  currentEditorFilesAtom,
  currentParserDbAtom,
  fileDiagnostics,
  functionTestCaseAtom,
  unsavedChangesAtom,
} from '../_atoms/atoms'
import { BAML_DIR } from '@/lib/constants'
import { atomStore } from '@/app/_components/JotaiProvider'
import { BAML, theme } from '@baml/codemirror-lang'
import { ParserDatabase } from '@baml/common'
import { Diagnostic, linter, lintGutter, openLintPanel } from '@codemirror/lint'
import CodeMirror, { EditorView } from '@uiw/react-codemirror'
import { BAMLProject } from '@/lib/exampleProjects'
import Link from 'next/link'

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

async function bamlLinter(_view: any): Promise<Diagnostic[]> {
  const lint = await import('@gloo-ai/baml-schema-wasm-web').then((m) => m.lint)
  const currentFiles = atomStore.get(currentEditorFilesAtom) as EditorFile[]
  const selectedTests = atomStore.get(functionTestCaseAtom) as Record<string, string>
  const linterInput: LinterInput = {
    root_path: `${BAML_DIR}`,
    files: currentFiles,
    selected_tests: selectedTests,
  }

  const res = lint(JSON.stringify(linterInput))
  const parsedRes = JSON.parse(res) as LintResponse
  const BamlDB = new Map<string, any>()
  BamlDB.set('baml_src', res)

  if (parsedRes.ok) {
    atomStore.set(currentParserDbAtom, parsedRes.response)
  }

  const allDiagnostics = parsedRes.diagnostics.map((d) => {
    return {
      from: d.start,
      to: d.end,
      message: d.text,
      severity: d.is_warning ? 'warning' : ('error' as Diagnostic['severity']),
      source: d.source_file,
    }
  })

  atomStore.set(fileDiagnostics, allDiagnostics)

  return allDiagnostics.filter((d) => d.source === atomStore.get(activeFileAtom)?.path)
}
const extensions = [
  BAML(),
  EditorView.lineWrapping,

  // lintGutter({}),

  linter(bamlLinter, {
    delay: 200,
    // needsRefresh: (update) => {

    // },
  }),
]

export const CodeMirrorEditor = ({ project }: { project: BAMLProject }) => {
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const [activeFile, setActiveFile] = useAtom(activeFileAtom)

  useEffect(() => {
    setActiveFile(project.files[0])
  }, [project.id])

  const setUnsavedChanges = useSetAtom(unsavedChangesAtom)

  return (
    <div className="w-full">
      <div className="flex px-3 py-1 h-fit gap-x-6 overflow-clip min-h-[20px]">
        <>
          {editorFiles
            .filter((f) => f.path === activeFile?.path)
            .map((file) => (
              <Button
                variant={'ghost'}
                key={file.path}
                onClick={() => setActiveFile(file)}
                className={`${
                  activeFile?.path === file.path
                    ? '  border-b-[2px] border-b-blue-400 bg-background text-blue-500 hover:bg-vscode-selection-background hover:text-blue-500'
                    : 'hover:text-black/80 bg-background text-gray-500 hover:bg-vscode-selection-background hover:text-gray-5=400'
                }  h-[20px] rounded-b-none rounded-tl-lg  border-r-0 px-1 text-xs  font-medium`}
              >
                {file.path.replace(`${BAML_DIR}/`, '')}
              </Button>
            ))}
        </>
        <div className="flex items-center justify-center h-full pt-0.5 pl-4">
          <Link
            href="https://docs.boundaryml.com"
            target="_blank"
            className="text-xs hover:text-foreground text-muted-foreground "
          >
            What is BAML?
          </Link>
        </div>
      </div>
      <div
        style={{
          height: 'calc(100% - 30px)',
        }}
      >
        <CodeMirror
          key={editorFiles.map((f) => f.path).join('')}
          value={activeFile?.content ?? ''}
          extensions={extensions}
          theme={theme}
          className="text-sm"
          height="100%"
          width="100%"
          maxWidth="100%"
          style={{ width: '100%', height: '100%' }}
          onChange={async (val, viewUpdate) => {
            setEditorFiles((prev) => {
              if (!activeFile) {
                return prev
              }
              const files = prev as EditorFile[] // because of jotai jsonstorage this becomes a promise or a normal object and this isnt a promise.
              if (!activeFile) {
                return files
              }
              const fileIndex = files.findIndex((file) => file.path === activeFile.path)

              const updatedFile: EditorFile = {
                path: activeFile.path,
                content: val,
              }

              // Update the file in place if it exists
              if (fileIndex !== -1) {
                files[fileIndex] = updatedFile
              } else {
                files.push(updatedFile)
              }

              // Return a new array to ensure React state update triggers re-render.
              return [...files]
            })
            window.history.replaceState(null, '', '/')
            setUnsavedChanges(true)
          }}
        />
      </div>
    </div>
  )
}
