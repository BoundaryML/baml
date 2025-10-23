import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { sessionStore } from '../../../../../baml_wasm_web/JotaiProvider';
import type { TestState } from '../../atoms';
import type { WatchNotification, CategorizedNotifications } from './types';

export interface TestHistoryEntry {
  timestamp: number;
  functionName: string;
  testName: string;
  response: TestState;
  input?: any;
}

export interface TestHistoryRun {
  timestamp: number;
  tests: TestHistoryEntry[];
}

// TODO: make this persistent, but make sure to serialize the wasm objects properly
export const testHistoryAtom = atom<TestHistoryRun[]>([]);
export const selectedHistoryIndexAtom = atom<number>(0);
export const isParallelTestsEnabledAtom = atomWithStorage<boolean>(
  'runTestsInParallel',
  true,
  sessionStore,
);

// Atom for current test's watch notifications
export const currentWatchNotificationsAtom = atom<WatchNotification[]>([])

// Derived atom for categorized notifications
export const categorizedNotificationsAtom = atom<CategorizedNotifications>((get) => {
  const notifications = get(currentWatchNotificationsAtom)

  return {
    variables: notifications.filter(n => n.variable_name && !n.is_stream),
    blocks: notifications.filter(n => n.value.startsWith('Block(')),
    streams: notifications.filter(n => n.is_stream),
  }
})
