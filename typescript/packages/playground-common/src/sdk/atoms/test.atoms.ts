/**
 * Test Execution State Atoms
 *
 * Manages state for test execution, history, and UI updates during test runs.
 */

import { atom } from 'jotai';
import type { WatchNotification, TestResponseData } from '../interface';

// Re-export WatchNotification for backward compatibility
export type { WatchNotification };

// ============================================================================
// Test State Types
// ============================================================================

/**
 * Test state during execution
 */
export type TestState =
  | { status: 'queued' }
  | { status: 'running'; response?: TestResponseData | string; watchNotifications?: WatchNotification[] }
  | {
      status: 'done';
      response: TestResponseData;
      response_status: 'passed' | 'llm_failed' | 'parse_failed' | 'constraints_failed' | 'assert_failed' | 'error';
      latency_ms: number;
      watchNotifications?: WatchNotification[];
    }
  | { status: 'error'; message: string };

/**
 * Individual test history entry
 */
export interface TestHistoryEntry {
  timestamp: number;
  functionName: string;
  testName: string;
  response: TestState;
  input?: any;
}

/**
 * A single test run containing multiple test executions
 */
export interface TestHistoryRun {
  timestamp: number;
  tests: TestHistoryEntry[];
}

/**
 * Code ranges to flash/highlight during execution
 */
export interface FlashRange {
  filePath: string;
  startLine: number;
  startCol: number;
  endLine: number;
  endCol: number;
}

// ============================================================================
// Test History Atoms
// ============================================================================

/**
 * Test execution history
 * Most recent runs first
 */
export const testHistoryAtom = atom<TestHistoryRun[]>([]);

/**
 * Currently selected test history index (0 = most recent)
 */
export const selectedHistoryIndexAtom = atom<number>(0);

/**
 * Currently selected test history run (derived)
 */
export const selectedTestHistoryAtom = atom((get) => {
  const history = get(testHistoryAtom);
  const index = get(selectedHistoryIndexAtom);
  return history[index] || null;
});

// ============================================================================
// Execution State Atoms
// ============================================================================

/**
 * Whether tests are currently running
 */
export const areTestsRunningAtom = atom<boolean>(false);

/**
 * Current abort controller for test execution
 */
export const currentAbortControllerAtom = atom<AbortController | null>(null);

// ============================================================================
// Watch Notifications & Highlighting
// ============================================================================

/**
 * Watch notifications for currently running test
 */
export const currentWatchNotificationsAtom = atom<WatchNotification[]>([]);

/**
 * Highlighted blocks from watch notifications
 */
export const highlightedBlocksAtom = atom<Set<string>>(new Set<string>());

/**
 * Code ranges to flash/highlight during execution
 */
export const flashRangesAtom = atom<FlashRange[]>([]);

/**
 * Categorized notifications (derived)
 */
export interface CategorizedNotifications {
  blocks: WatchNotification[];
  streams: WatchNotification[];
  regular: WatchNotification[];
}

export const categorizedNotificationsAtom = atom<CategorizedNotifications>((get) => {
  const notifications = get(currentWatchNotificationsAtom);

  const isBlock = (notification: WatchNotification) => {
    try {
      const parsed = JSON.parse(notification.value) as { type?: string } | undefined;
      if (parsed?.type === 'block') return true;
    } catch {}
    return notification.value.startsWith('Block(');
  };

  const isStream = (notification: WatchNotification) => {
    if (notification.isStream) return true;
    return notification.value.startsWith('Stream(');
  };

  return {
    blocks: notifications.filter(isBlock),
    streams: notifications.filter(isStream),
    regular: notifications.filter((n) => !isBlock(n) && !isStream(n)),
  };
});

// ============================================================================
// Pending Test Command
// ============================================================================

/**
 * Pending test command to execute after runtime initialization
 * Used when a run_test codelens is received before the runtime is ready
 */
export interface PendingTestCommand {
  functionName: string;
  testName: string;
  timestamp: number;
}

export const pendingTestCommandAtom = atom<PendingTestCommand | null>(null);
