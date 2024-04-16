'use client'
import { EditorFile } from '@/app/actions'
import { Button } from '@baml/playground-common/components/ui/button'
import { useAtom, useSetAtom } from 'jotai'
import { usePathname } from 'next/navigation'
import { useState, useEffect } from 'react'
import { currentEditorFilesAtom, currentParserDbAtom, unsavedChangesAtom } from '../_atoms/atoms'
import { BAML_DIR } from '@/lib/constants'
import { atomStore } from '@/app/_components/JotaiProvider'
import { BAML, theme } from '@baml/codemirror-lang'
import { ParserDatabase } from '@baml/common'
import { Diagnostic, linter } from '@codemirror/lint'
import CodeMirror, { EditorView } from '@uiw/react-codemirror'
import { BAMLProject } from '@/lib/exampleProjects'

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
}
async function bamlLinter(view: EditorView): Promise<Diagnostic[]> {
  const lint = await import('@gloo-ai/baml-schema-wasm-web').then((m) => m.lint)
  const currentFiles = atomStore.get(currentEditorFilesAtom) as EditorFile[]
  const linterInput: LinterInput = {
    root_path: `${BAML_DIR}`,
    files: currentFiles,
  }
  console.info(`Linting ${linterInput.files.length} files in ${linterInput.root_path}`)
  const res = lint(JSON.stringify(linterInput))
  const parsedRes = JSON.parse(res) as LintResponse
  const BamlDB = new Map<string, any>()
  BamlDB.set('baml_src', res)

  if (parsedRes.ok) {
    const newParserDb: ParserDatabase = { ...parsedRes.response }
    atomStore.set(currentParserDbAtom, newParserDb)
  }

  return parsedRes.diagnostics.map((d) => {
    return {
      from: d.start,
      to: d.end,
      message: d.text,
      severity: d.is_warning ? 'warning' : 'error',
    }
  })
}
const extensions = [
  BAML(),
  EditorView.lineWrapping,
  linter(bamlLinter, {
    delay: 200,
    // needsRefresh: (view) => ,
  }),
]

export const CodeMirrorEditor = ({ project }: { project: BAMLProject }) => {
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const [activeFile, setActiveFile] = useState<EditorFile>(project.files[0])

  const setUnsavedChanges = useSetAtom(unsavedChangesAtom)

  return (
    <div className="w-full">
      <div className="border-border flex h-fit gap-x-3 overflow-clip rounded-t-lg border-x-[1px] border-t-[1px]  px-3 py-1">
        {editorFiles.map((file) => (
          <Button
            variant={'ghost'}
            key={file.path}
            onClick={() => setActiveFile(file)}
            className={`${
              activeFile?.path === file.path
                ? '  border-b-[2px] border-b-blue-400 bg-background text-blue-500 hover:bg-vscode-selection-background hover:text-blue-500'
                : 'hover:text-black/80 bg-background text-gray-500 hover:bg-vscode-selection-background hover:text-gray-5=400'
            }  h-[30px] rounded-b-none rounded-tl-lg  border-r-0 px-1 text-sm  font-medium`}
          >
            {file.path.replace(`${BAML_DIR}/`, '')}
          </Button>
        ))}
      </div>
      <>
        <CodeMirror
          value={activeFile?.content ?? ''}
          extensions={extensions}
          theme={theme}
          height="100%"
          width="100%"
          maxWidth="100%"
          style={{ width: '100%', height: '100%' }}
          onChange={async (val, viewUpdate) => {
            setEditorFiles((prev) => {
              const files = prev as EditorFile[] // because of jotai jsonstorage this becomes a promise or a normal object and this isnt a promise.
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
      </>
    </div>
  )
}
