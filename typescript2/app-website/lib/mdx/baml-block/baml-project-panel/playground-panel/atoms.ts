import { atom } from 'jotai';
import { runtimeAtom } from '../atoms';

export const runtimeStateAtom = atom((get) => {
  const { rt } = get(runtimeAtom);
  if (rt === undefined) {
    return { functions: [] };
  }
  const functions = rt.list_functions();

  return { functions };
});

const selectedFunctionAtom = atom<string | undefined>(undefined);
const selectedTestcaseAtom = atom<string | undefined>(undefined);

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

export const selectionAtom = atom((get) => {
  const selectedFunction = get(selectedFunctionAtom);
  const selectedTestcase = get(selectedTestcaseAtom);

  const state = get(runtimeStateAtom);

  let selectedFn = state.functions.at(0);
  if (selectedFunction !== undefined) {
    const foundFn = state.functions.find((f) => f.name === selectedFunction);
    if (foundFn) {
      selectedFn = foundFn;
    }
  }

  let selectedTc = selectedFn?.test_cases.at(0);
  if (selectedTestcase !== undefined) {
    const foundTc = selectedFn?.test_cases.find(
      (tc) => tc.name === selectedTestcase,
    );
    if (foundTc) {
      selectedTc = foundTc;
    }
  }

  return { selectedFn, selectedTc };
});

export interface SelectedItem {
  functionName: string;
  testName: string;
}

export const selectedItemsAtom = atom<SelectedItem[]>([]);

// Related to test status
import {
  type WasmFunctionResponse,
  type WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';
import { atomFamily } from 'jotai/utils';

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
  (params: { functionName: string; testName: string }) =>
    atom((get) => {
      const allTestCaseResponse = get(runningTestsAtom);
      const testCaseResponse = allTestCaseResponse.find(
        (t) =>
          t.functionName === params.functionName &&
          t.testName === params.testName,
      );
      return testCaseResponse?.state;
    }),
);
export const areTestsRunningAtom = atom(false);
// TODO: this is never set.
export const runningTestsAtom = atom<
  { functionName: string; testName: string; state: TestState }[]
>([]);
