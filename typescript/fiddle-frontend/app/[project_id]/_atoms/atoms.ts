import { EditorFile } from "@/app/actions"
// import { ParserDBFunctionTestModel } from "@/lib/exampleProjects"
import { ParserDatabase, TestState } from "@baml/common"
import { projectFilesAtom, updateFileAtom } from "@baml/playground-common"
import { Diagnostic } from "@codemirror/lint"
import { atom } from "jotai"
import { atomWithStorage } from "jotai/utils"


export const project_root = "baml_src";
export const currentEditorFilesAtom = atom(
  (get) => {
    const files = get(projectFilesAtom(project_root));
    return Object.entries(files).map(([path, content]): EditorFile => ({
      path, content
    }));
  }
);
export const currentParserDbAtom = atom<ParserDatabase | null>(null)

// Name of the function -> Name of currently rendered test
export const functionTestCaseAtom = atom<{ [key: string]: string }>({})


// export const functionsAndTestsAtom = atomWithStorage<ParserDBFunctionTestModel[]>(
//   'parserdb_functions',
//   [],
//   sessionStore as any,
// )
export const testRunOutputAtom = atom<TestRunOutput | null>(null)
export const unsavedChangesAtom = atom<boolean>(false);

export const activeFileNameAtom = atom<string | null>(null);
export const activeFileAtom = atom((get) => {
  const files = get(currentEditorFilesAtom);
  const activeFileName = get(activeFileNameAtom);
  return files.find(f => f.path === activeFileName);
})

export const fileDiagnostics = atom<Diagnostic[]>([]);
export const emptyDirsAtom = atom<string[]>([]);
export const exploreProjectsOpenAtom = atom<boolean>(false);


export const productTourDoneAtom = atomWithStorage<boolean>('initial_tutorial_v1', false);
export const productTourTestDoneAtom = atomWithStorage<boolean>('test_tour_v1', false);


export type TestRunOutput = {
  testState: TestState;
  outputLogs: string[];
}