import { sessionStore } from "@/app/_components/JotaiProvider"
import { EditorFile } from "@/app/actions"
import { ParserDBFunctionTestModel } from "@/lib/exampleProjects"
import { ParserDatabase } from "@baml/common"
import { atom } from "jotai"
import { atomWithStorage } from "jotai/utils"

export const currentParserDbAtom = atom<ParserDatabase | null>(null)
export const currentEditorFilesAtom = atomWithStorage<EditorFile[]>('files', [], sessionStore as any)
export const functionsAndTestsAtom = atomWithStorage<ParserDBFunctionTestModel[]>(
  'parserdb_functions',
  [],
  sessionStore as any,
)