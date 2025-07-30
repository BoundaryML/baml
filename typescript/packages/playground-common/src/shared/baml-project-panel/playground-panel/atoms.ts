import { type Atom, atom } from 'jotai';
import { runtimeAtom } from '../atoms';

// Related to test status
import type {
  WasmFunction,
  WasmFunctionResponse,
  WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';
import { atomFamily } from 'jotai/utils';

export const runtimeStateAtom: Atom<{
  functions: WasmFunction[];
  stale: boolean;
}> = atom((get) => {
  const { rt, lastValidRt } = get(runtimeAtom);
  console.debug('rt', rt);
  if (rt === undefined) {
    if (lastValidRt === undefined) {
      return { functions: [], stale: false };
    }
    return { functions: lastValidRt.list_functions(), stale: true };
  }
  const functions = rt.list_functions();
  return { functions, stale: false };
});

export const selectedFunctionAtom = atom<string | undefined>(undefined);
export const selectedTestcaseAtom = atom<string | undefined>(undefined);

export const selectedItemAtom = atom(
  (get) => {
    const selected = get(selectionAtom);
    if (
      selected.selectedFn === undefined ||
      selected.selectedTc === undefined
    ) {
      return undefined;
    }
    return [selected.selectedFn.name, selected.selectedTc.name] as [
      string,
      string,
    ];
  },
  (_, set, functionName: string, testcaseName: string) => {
    set(selectedFunctionAtom, functionName);
    set(selectedTestcaseAtom, testcaseName);
  },
);

export const functionObjectAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom);
    const fn = functions.find((f) => f.name === functionName);
    if (!fn) {
      return undefined;
    }
    return fn;
  }),
);

export const testcaseObjectAtom = atomFamily(
  (params: { functionName: string; testcaseName?: string | null }) =>
    atom((get) => {
      const { functions } = get(runtimeStateAtom);
      const fn = functions.find((f) => f.name === params.functionName);
      if (!fn) {
        return undefined;
      }
      const tc = fn.test_cases.find((tc) => tc.name === params.testcaseName);
      if (!tc) {
        return undefined;
      }
      return tc;
    }),
);

export const updateCursorAtom = atom(
  null,
  (
    get,
    set,
    cursor: {
      fileName: string;
      fileText: string;
      line: number;
      column: number;
    },
  ) => {
    const runtime = get(runtimeAtom)?.rt;

    if (runtime) {
      const fileName = cursor.fileName;
      const fileContent = cursor.fileText;
      const lines = fileContent.split('\n');

      let cursorIdx = 0;
      for (let i = 0; i < cursor.line - 1; i++) {
        cursorIdx += (lines[i]?.length ?? 0) + 1; // +1 for the newline character
      }

      cursorIdx += cursor.column;

      const currentSelectedFunction = get(selectedFunctionAtom);
      const currentSelectedTestcase = get(selectedTestcaseAtom);

      const selectedFunc = runtime.get_function_at_position(
        fileName,
        currentSelectedFunction ?? '',
        cursorIdx,
      );

      if (selectedFunc) {
        const functionChanged = selectedFunc.name !== currentSelectedFunction;
        set(selectedFunctionAtom, selectedFunc.name);

        // Check if cursor is inside a specific test case
        const selectedTestcase = runtime.get_testcase_from_position(
          selectedFunc,
          cursorIdx,
        );

        if (selectedTestcase) {
          // If cursor is inside a test case, always use that test case
          set(selectedTestcaseAtom, selectedTestcase.name);

          // Check if this test case belongs to a different function
          const nestedFunc = runtime.get_function_of_testcase(
            fileName,
            cursorIdx,
          );

          if (nestedFunc) {
            set(selectedFunctionAtom, nestedFunc.name);
          }
        } else if (functionChanged) {
          // Function changed and cursor is not in a test case
          const { functions } = get(runtimeStateAtom);
          const newFunc = functions.find((f) => f.name === selectedFunc.name);
          if (newFunc && newFunc.test_cases.length > 0) {
            // Reset to first test case of the new function
            set(selectedTestcaseAtom, newFunc.test_cases[0]?.name);
          } else {
            // Function has no test cases, set to undefined
            set(selectedTestcaseAtom, undefined);
          }
        }
        // If function didn't change and cursor is not in a test case,
        // preserve the current test case selection
      }
    }
  },
);

export const selectionAtom = atom((get) => {
  const selectedFunction = get(selectedFunctionAtom);
  const selectedTestcase = get(selectedTestcaseAtom);

  const state = get(runtimeStateAtom);

  // Get the selected function, defaulting to first if none selected
  let selectedFn = state.functions.at(0);
  if (selectedFunction !== undefined) {
    const foundFn = state.functions.find((f) => f.name === selectedFunction);
    if (foundFn) {
      selectedFn = foundFn;
    } else {
      // Function not found, fallback to first function
      console.debug(
        'Function not found, using first function',
        selectedFunction,
      );
      selectedFn = state.functions.at(0);
    }
  }

  // Get the selected test case
  let selectedTc = undefined;
  if (selectedFn) {
    if (selectedTestcase !== undefined) {
      // Try to find the selected test case in current function
      const foundTc = selectedFn.test_cases.find(
        (tc) => tc.name === selectedTestcase,
      );
      if (foundTc) {
        selectedTc = foundTc;
      } else {
        // Test case not found in current function, use first test case
        console.debug(
          'Testcase not found in current function, using first',
          selectedTestcase,
        );
        selectedTc = selectedFn.test_cases.at(0);
      }
    } else {
      // No test case selected, use first one
      selectedTc = selectedFn.test_cases.at(0);
    }
  }

  return { selectedFn, selectedTc };
});

export const selectedFunctionObjectAtom = atom((get) => {
  const { selectedFn } = get(selectionAtom);
  return selectedFn;
});

export type TestStatusType = 'queued' | 'running' | 'done' | 'error' | 'idle';
export type DoneTestStatusType =
  | 'passed'
  | 'llm_failed'
  | 'parse_failed'
  | 'constraints_failed'
  | 'assert_failed'
  | 'error';
export type TestState =
  | {
      status: 'queued' | 'idle';
    }
  | {
      status: 'running';
      response?: WasmFunctionResponse;
    }
  | {
      status: 'done';
      response_status: DoneTestStatusType;
      response: WasmTestResponse;
      latency_ms: number;
    }
  | {
      status: 'error';
      message: string;
    };

export const testCaseAtom = atomFamily(
  (params: { functionName: string; testName: string }) =>
    atom((get) => {
      const { functions } = get(runtimeStateAtom);
      const fn = functions.find((f) => f.name === params.functionName);
      const tc = fn?.test_cases.find((tc) => tc.name === params.testName);
      if (!fn || !tc) {
        return undefined;
      }
      return { fn, tc };
    }),
);

export const functionTestSnippetAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom);
    const fn = functions.find((f) => f.name === functionName);
    if (!fn) {
      return undefined;
    }
    return fn.test_snippet;
  }),
);

export const testCaseResponseAtom = atomFamily(
  (params: { functionName?: string; testName?: string }) =>
    atom((get) => {
      const allTestCaseResponse = get(runningTestsAtom);
      const testCaseResponse = allTestCaseResponse.find(
        (t) =>
          t.functionName === params.functionName &&
          t.testName === params.testName,
        undefined,
      );
      return testCaseResponse?.state;
    }),
);
export const areTestsRunningAtom = atom(false);
// TODO: this is never set.
export const runningTestsAtom = atom<
  { functionName: string; testName: string; state: TestState }[]
>([]);

export interface FlashRange {
  filePath: string;
  startLine: number;
  startCol: number;
  endLine: number;
  endCol: number;
}

export const flashRangesAtom = atom<FlashRange[]>([]);
