import type { EditorFile } from '@/app/actions'
// import { ParserDBFunctionTestModel } from "@/lib/exampleProjects"
import { TestState } from '@baml/common'
import { availableFunctionsAtom, selectedFunctionAtom } from '@baml/playground-common/baml_wasm_web/EventListener'
import { sessionStore } from '@baml/playground-common/baml_wasm_web/JotaiProvider'
import { projectFilesAtom } from '@baml/playground-common/baml_wasm_web/baseAtoms'
import { Diagnostic } from '@codemirror/lint'
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export const PROJECT_ROOT = 'baml_src'
export const currentEditorFilesAtom = atom((get) => {
  return Object.entries(get(projectFilesAtom(PROJECT_ROOT))).map(([path, content]): EditorFile => {
    return { path, content }
  })
})

// export const functionsAndTestsAtom = atomWithStorage<ParserDBFunctionTestModel[]>(
//   'parserdb_functions',
//   [],
//   sessionStore as any,
// )
export const unsavedChangesAtom = atom<boolean>(false)
const activeFileNameAtomRaw = atomWithStorage<string | null>('active_file', null, sessionStore)
export const activeFileNameAtom = atom(
  (get) => {
    const files = get(currentEditorFilesAtom)
    let activeFileName = get(activeFileNameAtomRaw)
    // Validate the current active file or determine a new one
    if (!activeFileName || !files.some((f) => f.path === activeFileName)) {
      const defaultFile = 'baml_src/01-extract-receipt.baml'
      const excludeFile = 'baml_src/clients.baml'
      const alternativeFiles = files.filter((f) => f.path !== excludeFile).sort((a, b) => a.path.localeCompare(b.path))

      // 1. Default file if available
      // 2. First non-excluded file, sorted alphabetically
      // 3. First file in the list as a last resort
      activeFileName = files.find((f) => f.path === defaultFile)?.path || alternativeFiles[0]?.path || files[0]?.path
    }

    // Find and return the selected file path or null if none are valid
    const selectedFile = files.find((f) => f.path === activeFileName)
    return selectedFile ? selectedFile.path : null
  },
  (get, set, path: string) => {
    const files = get(currentEditorFilesAtom)
    if (files.some((f) => f.path === path)) {
      set(activeFileNameAtomRaw, path)
    }
  },
)

export const activeFileContentAtom = atom((get) => {
  const files = get(currentEditorFilesAtom)
  const activeFileName = get(activeFileNameAtom)
  const selectedFile = files.find((f) => f.path === activeFileName)
  return selectedFile?.content ?? ''
})

export const emptyDirsAtom = atom<string[]>([])
export const exploreProjectsOpenAtom = atom<boolean>(false)
export const libraryOpenAtom = atom<boolean>(false)

export const productTourDoneAtom = atomWithStorage<boolean>('initial_tutorial_v1', false)
export const productTourTestDoneAtom = atomWithStorage<boolean>('test_tour_v1', false)
