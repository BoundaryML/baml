import { sessionStore } from "@/app/_components/JotaiProvider"
import { EditorFile } from "@/app/actions"
import { ParserDBFunctionTestModel } from "@/lib/exampleProjects"
import { ParserDatabase, TestState } from "@baml/common"
import { atom } from "jotai"
import { atomWithStorage } from "jotai/utils"

export const currentParserDbAtom = atom<ParserDatabase | null>(null)
export const currentEditorFilesAtom = atomWithStorage<EditorFile[]>('files', [], sessionStore as any)
export const functionsAndTestsAtom = atomWithStorage<ParserDBFunctionTestModel[]>(
  'parserdb_functions',
  [],
  sessionStore as any,
)
export const testRunOutputAtom = atom<TestRunOutput | null>(null)
export const unsavedChangesAtom = atom<boolean>(false);

export type TestRunOutput = {
  testState: TestState;
  outputLogs: string[];
}