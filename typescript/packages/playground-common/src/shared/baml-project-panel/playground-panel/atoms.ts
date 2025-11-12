/**
 * Playground Panel Atoms
 *
 * This file bridges between the old WASM-based runtime and the new SDK.
 * It maintains backward compatibility while using SDK atoms where possible.
 */

import { type Atom, atom } from 'jotai';
import { filesAtom, runtimeAtom } from '../atoms';

// Related to test status
import type {
  WasmFunction,
  WasmFunctionResponse,
  WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';
import { atomFamily, atomWithStorage } from 'jotai/utils';
import type { NavigationIntent } from '../../../sdk/types';

// ============================================================================
// SDK Atoms - Direct Re-exports
// ============================================================================

// Re-export SDK atoms directly without creating local copies
export {
  // Test execution state
  areTestsRunningAtom,
  currentAbortControllerAtom,
  flashRangesAtom,
  testHistoryAtom,
  selectedHistoryIndexAtom,
  selectedTestHistoryAtom,
  currentWatchNotificationsAtom,
  highlightedBlocksAtom,
  categorizedNotificationsAtom,

  // Types
  type TestState,
  type TestHistoryEntry,
  type TestHistoryRun,
  type WatchNotification,
  type FlashRange,
  type CategorizedNotifications,
} from '../../../sdk/atoms/test.atoms';

// Import for internal use
import {
  selectedTestHistoryAtom,
} from '../../../sdk/atoms/test.atoms';

// Re-export selection atoms from SDK
import {
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  unifiedSelectionStateAtom,
  functionsAtom,
} from '../../../sdk/atoms/core.atoms';
import { navigationDispatcherAtom } from '../../../sdk/navigation/dispatcher';

type FunctionIntent = Extract<NavigationIntent, { type: 'function' }>;
type TestIntent = Extract<NavigationIntent, { type: 'test' }>;

const inferFunctionType = (fn: { type?: string; functionFlavor?: 'llm' | 'expr' } | null | undefined): FunctionIntent['functionType'] => {
  if (!fn) return 'function';
  if (fn.type && ['workflow', 'llm_function', 'function', 'conditional', 'loop', 'group', 'return', 'block'].includes(fn.type)) {
    return fn.type as FunctionIntent['functionType'];
  }
  if (fn.functionFlavor === 'llm') {
    return 'llm_function';
  }
  return 'function';
};

const nodeTypeForFunction = (functionType: FunctionIntent['functionType']): TestIntent['nodeType'] =>
  functionType === 'llm_function' ? 'llm_function' : 'function';

export const graphControlsTipDismissedAtom = atomWithStorage(
  'playground:graphControlsTipDismissed',
  false
);

// ============================================================================
// Derived Selection Atoms
// ============================================================================

export const selectedItemAtom = atom(
  (get) => {
    const selected = get(selectionAtom);
    if (
      selected.selectedFn === null ||
      selected.selectedTc === null
    ) {
      return undefined;
    }
    return [selected.selectedFn.name, selected.selectedTc.name] as [
      string,
      string,
    ];
  },
  (_get, _set, _functionName: string, _testcaseName: string | undefined) => {
    throw new Error(
      'selectedItemAtom setter is deprecated. Use navigationDispatcherAtom directly instead. ' +
      'See function-item.tsx and test-item.tsx for examples.'
    );
  },
);

// ============================================================================
// Function & Test Case Helpers
// ============================================================================

export const functionObjectAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const functions = get(functionsAtom);
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
      const functions = get(functionsAtom);
      const fn = functions.find((f) => f.name === params.functionName);
      if (!fn) {
        return undefined;
      }
      const tc = fn.testCases?.find((tc) => tc.name === params.testcaseName);
      if (!tc) {
        return undefined;
      }
      return tc;
    }),
);

// ============================================================================
// Cursor Management
// ============================================================================

/**
 * Update cursor position - determines which function/test is at cursor and updates selection
 *
 * NOTE: This is a legacy atom for backward compatibility.
 * The logic has been abstracted into BamlRuntime.updateCursor() and SDK.cursor.update()
 * This atom now uses the runtime's method instead of calling WASM directly.
 */
export const updateCursorAtom = atom(
  null,
  (
    get,
    set,
    cursor: {
      fileName: string;
      line: number;
      column: number;
    },
  ) => {
    const runtime = get(runtimeAtom)?.rt;
    if (!runtime) {
      return;
    }
    const fileContent = get(filesAtom)[cursor.fileName];
    if (!fileContent) {
      return;
    }

    const fileName = cursor.fileName;
    const lines = fileContent.split('\n');

    let cursorIdx = 0;
    for (let i = 0; i < cursor.line; i++) {
      cursorIdx += (lines[i]?.length ?? 0) + 1; // +1 for the newline character
    }
    cursorIdx += cursor.column;

    const selectedFunc = runtime.get_function_at_position(
      fileName,
      get(selectedFunctionNameAtom) ?? '',
      cursorIdx,
    );

    if (selectedFunc) {
      const selectedTestcase = runtime.get_testcase_from_position(
        selectedFunc,
        cursorIdx,
      );

      if (selectedTestcase) {
        // Check for nested function in test case
        const nestedFunc = runtime.get_function_of_testcase(
          fileName,
          cursorIdx,
        );

        const targetFunction = nestedFunc ?? selectedFunc;
        const functionType = inferFunctionType(targetFunction as any);
        const nodeType = nodeTypeForFunction(functionType);

        set(navigationDispatcherAtom, {
          type: 'test',
          functionName: targetFunction.name,
          testName: selectedTestcase.name,
          nodeType,
          filePath: fileName,
          source: 'cursor',
        });
      } else {
        // Just a function, no test case
        const functionType = inferFunctionType(selectedFunc as any);

        set(navigationDispatcherAtom, {
          type: 'function',
          functionName: selectedFunc.name,
          functionType,
          filePath: fileName,
          source: 'cursor',
        });
      }
    }
  }
);

// ============================================================================
// Selection State
// ============================================================================

export const selectionAtom = atom((get) => {
  const selectedFunction = get(selectedFunctionNameAtom);
  const selectedTestcase = get(selectedTestCaseNameAtom);

  const functions = get(functionsAtom);

  type FunctionType = (typeof functions)[number];
  let selectedFn: FunctionType | null = null;
  if (selectedFunction !== null) {
    const foundFn = functions.find((f) => f.name === selectedFunction);
    if (foundFn) {
      selectedFn = foundFn;
    } else {
      console.warn('Function not found', selectedFunction);
    }
  }

  type TestType = FunctionType['testCases'][number];
  let selectedTc: TestType | null = null;
  if (selectedFn && selectedTestcase !== null) {
    selectedTc = selectedFn.testCases?.find((tc) => tc.name === selectedTestcase) ?? null;
    if (!selectedTc) {
      console.warn('Testcase not found', selectedTestcase);
    }
  }

  return { selectedFn, selectedTc };
});

export const selectedFunctionObjectAtom = atom((get) => {
  const { selectedFn } = get(selectionAtom);
  return selectedFn;
});

// ============================================================================
// Test Status Types (for backward compatibility)
// ============================================================================

export type TestStatusType = 'queued' | 'running' | 'done' | 'error' | 'idle';
export type DoneTestStatusType =
  | 'passed'
  | 'llm_failed'
  | 'parse_failed'
  | 'constraints_failed'
  | 'assert_failed'
  | 'error';

// ============================================================================
// Test Case Helpers
// ============================================================================

export const testCaseAtom = atomFamily(
  (params: { functionName: string; testName: string }) =>
    atom((get) => {
      const functions = get(functionsAtom);
      const fn = functions.find((f) => f.name === params.functionName);
      const tc = fn?.testCases?.find((tc) => tc.name === params.testName);
      if (!fn || !tc) {
        return undefined;
      }
      return { fn, tc };
    }),
);

export const functionTestSnippetAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const functions = get(functionsAtom);
    const fn = functions.find((f) => f.name === functionName);
    if (!fn) {
      return undefined;
    }
    return fn.testSnippet;
  }),
);

// ============================================================================
// Test Case Response (uses SDK test history)
// ============================================================================

/**
 * Get the test state for a specific function/test case from SDK test history
 * This replaces the old runningTestsAtom which was never set
 */
export const testCaseResponseAtom = atomFamily(
  (params: { functionName?: string; testName?: string }) =>
    atom((get) => {
      // Get the currently selected test history run (most recent)
      const historyRun = get(selectedTestHistoryAtom);
      if (!historyRun) {
        return undefined;
      }

      // Find the matching test in the history
      const testEntry = historyRun.tests.find(
        (t) =>
          t.functionName === params.functionName &&
          t.testName === params.testName
      );

      // Return the test state (response field contains the TestState)
      return testEntry?.response;
    }),
);

// ============================================================================
// UNIFIED STATE INTEGRATION
// ============================================================================

// Re-export unified atoms
export {
  unifiedSelectionAtom,
  activeTabAtom,
  viewModeAtom,
  bottomPanelModeAtom,
  shouldShowGraphAtom,
  type UnifiedSelection,
  type TabValue,
  type BottomPanelMode,
} from './unified-atoms';
