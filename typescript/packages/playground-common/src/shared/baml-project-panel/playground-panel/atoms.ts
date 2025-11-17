/**
 * Playground Panel Atoms
 *
 * This file bridges between the old WASM-based runtime and the new SDK.
 * It maintains backward compatibility while using SDK atoms where possible.
 */

import { type Atom, atom } from 'jotai';

import { atomFamily, atomWithStorage } from 'jotai/utils';

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

type FunctionType = 'workflow' | 'function' | 'llm_function' | 'conditional' | 'loop' | 'group' | 'return' | 'block';
type NodeType = 'llm_function' | 'function';



export const graphControlsTipDismissedAtom = atomWithStorage(
  'playground:graphControlsTipDismissed',
  false
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
      try {
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
      } catch (error) {
        console.error('Error getting testcase object', error);
        return undefined;
      }
    }),
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
