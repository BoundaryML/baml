/**
 * Test Panel Atoms
 *
 * This file re-exports SDK test atoms and provides UI-specific atoms for the test panel.
 * Most test execution state is managed by the SDK - we just add UI preferences here.
 */

import { atomWithStorage } from 'jotai/utils';
import { sessionStore } from '../../../../../baml_wasm_web/JotaiProvider';

// Re-export all test atoms from SDK
// The SDK manages test execution state - UI just reads from these atoms
export {
  testHistoryAtom,
  selectedHistoryIndexAtom,
  currentWatchNotificationsAtom,
  highlightedBlocksAtom,
  categorizedNotificationsAtom,
  areTestsRunningAtom,
} from '../../../../../sdk/atoms/test.atoms';

// Re-export test types from SDK
export type {
  TestHistoryEntry,
  TestHistoryRun,
  TestState,
  WatchNotification,
  CategorizedNotifications,
} from '../../../../../sdk/atoms/test.atoms';

// UI-specific atom for parallel test execution preference
export const isParallelTestsEnabledAtom = atomWithStorage<boolean>(
  'runTestsInParallel',
  true,
  sessionStore,
);
