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
    // hack to get default file selection for now..
    const activeFileName = get(activeFileNameAtomRaw) ?? 'baml_src/01-extract-receipt.baml'
    const selectedFile = files.find((f) => f.path === activeFileName) ?? files[0]

    if (selectedFile) {
      return selectedFile.path
    }
    return null
  },
  (get, set, path: string) => {
    const files = get(currentEditorFilesAtom)
    if (files.some((f) => f.path === path)) {
      set(activeFileNameAtomRaw, path)
      const availableFunctions = get(availableFunctionsAtom)
      // Set the default selected function when the current active file is changed
      const selectedFunction = availableFunctions.find((f) => f.span.file_path === path)
      if (selectedFunction) {
        set(selectedFunctionAtom, selectedFunction.name)
      }
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
