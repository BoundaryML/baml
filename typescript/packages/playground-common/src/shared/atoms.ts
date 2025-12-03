/**
 * Shared atoms used across the application
 *
 * Re-exports commonly used atoms from the SDK for convenience.
 */

import { atom } from 'jotai';

// Selection state
export {
  selectionAtom,
  selectedFunctionObjectAtom,
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  selectedTestCaseAtom,
  functionTestSnippetAtom,
  runtimeAtom,
} from '../sdk/atoms/core.atoms';

// Test execution state
export {
  areTestsRunningAtom,
  testHistoryAtom,
  selectedHistoryIndexAtom,
  selectedTestHistoryAtom,
  currentWatchNotificationsAtom,
  highlightedBlocksAtom,
  flashRangesAtom,
  categorizedNotificationsAtom,
  currentAbortControllerAtom,
} from '../sdk/atoms/test.atoms';

// Re-export types
export type {
  TestState,
  TestHistoryEntry,
  TestHistoryRun,
  WatchNotification,
  FlashRange,
  CategorizedNotifications,
} from '../sdk/atoms/test.atoms';

// Import atoms for aliases
import {
  selectionAtom,
  selectedFunctionObjectAtom,
  selectedTestCaseAtom,
  runtimeAtom,
  functionsAtom,
} from '../sdk/atoms/core.atoms';
import { selectedTestHistoryAtom } from '../sdk/atoms/test.atoms';

// ============================================================================
// Compatibility aliases for old code
// ============================================================================

/**
 * @deprecated Use selectedFunctionObjectAtom instead
 */
export const functionObjectAtom = selectedFunctionObjectAtom;

/**
 * @deprecated Use selectedTestCaseAtom instead
 */
export const testcaseObjectAtom = selectedTestCaseAtom;

/**
 * @deprecated Use selectionAtom and check selectedTc instead
 */
export const testCaseAtom = selectedTestCaseAtom;

/**
 * Type alias for backward compatibility
 * @deprecated Use TestState['response_status'] instead
 */
export type DoneTestStatusType = 'passed' | 'llm_failed' | 'parse_failed' | 'constraints_failed' | 'assert_failed' | 'error';

/**
 * Derived atom for test case response (from test history)
 * @deprecated Access test state from testHistoryAtom instead
 */
export const testCaseResponseAtom = atom((get) => {
  const history = get(selectedTestHistoryAtom);
  if (!history || !history.tests.length) return null;
  return history.tests[0]?.response || null;
});

// /**
//  * @deprecated Use SDK functionsAtom instead
//  * This is a compatibility shim for old code
//  */
// export const runtimeStateAtom = atom((get) => {
//   const runtime = get(runtimeAtom);

//   if (!runtime.rt) {
//     // No current runtime, check if we have a last valid one
//     if (!runtime.lastValidRt) {
//       return { functions: [], stale: false };
//     }
//     // Use last valid runtime (stale)
//     const llmFunctions = runtime.lastValidRt.list_functions?.() || [];
//     const exprFunctions = runtime.lastValidRt.list_expr_fns?.() || [];
//     return {
//       functions: [...llmFunctions, ...exprFunctions],
//       stale: true
//     };
//   }

//   // Current runtime is valid
//   const llmFunctions = runtime.rt.list_functions?.() || [];
//   const exprFunctions = runtime.rt.list_expr_fns?.() || [];
//   return {
//     functions: [...llmFunctions, ...exprFunctions],
//     stale: false
//   };
// });


/**
 * Current BAML files atom
 * Re-export for backward compatibility
 */
export { filesAtom } from '../sdk/atoms/core.atoms';

