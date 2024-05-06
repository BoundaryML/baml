import { atomStore, sessionStore } from '@/app/_components/JotaiProvider'
import { EditorFile } from '@/app/actions'
// import { ParserDBFunctionTestModel } from "@/lib/exampleProjects"
import { ParserDatabase, TestState } from '@baml/common'
import { Diagnostic } from '@codemirror/lint'
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export const currentParserDbAtom = atom<ParserDatabase | null>(null)
export const currentEditorFilesAtom = atomWithStorage<EditorFile[]>('files', [], sessionStore as any)
// Name of the function -> Name of currently rendered test
export const functionTestCaseAtom = atomWithStorage<{ [key: string]: string }>(
  'function_test_cases',
  {},
  sessionStore as any,
)
// export const functionsAndTestsAtom = atomWithStorage<ParserDBFunctionTestModel[]>(
//   'parserdb_functions',
//   [],
//   sessionStore as any,
// )
export const testRunOutputAtom = atom<TestRunOutput | null>(null)
export const unsavedChangesAtom = atom<boolean>(false)

export const activeFileAtom = atom<EditorFile | null>(null)
export const fileDiagnostics = atom<Diagnostic[]>([])
export const emptyDirsAtom = atom<string[]>([])
export const exploreProjectsOpenAtom = atom<boolean>(false)

export const productTourDoneAtom = atomWithStorage<boolean>('initial_tutorial_v1', false)
export const productTourTestDoneAtom = atomWithStorage<boolean>('test_tour_v1', false)

export type TestRunOutput = {
  testState: TestState
  outputLogs: string[]
}
