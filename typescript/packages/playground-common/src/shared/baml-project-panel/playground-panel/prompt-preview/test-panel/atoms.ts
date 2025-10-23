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
const currentWatchNotificationsBaseAtom = atom<WatchNotification[]>([])
export const currentWatchNotificationsAtom = atom(
  (get) => get(currentWatchNotificationsBaseAtom),
  (get, set, update: WatchNotification[] | ((prev: WatchNotification[]) => WatchNotification[])) => {
    const previous = get(currentWatchNotificationsBaseAtom)
    const next = typeof update === 'function' ? (update as (prev: WatchNotification[]) => WatchNotification[])(previous) : update

    const lastNotification = next[next.length - 1]
    let blockLabel: string | undefined
    if (lastNotification) {
      try {
        const parsed = JSON.parse(lastNotification.value) as { type?: string; label?: string } | undefined
        if (parsed?.type === 'block' && typeof parsed.label === 'string') {
          blockLabel = parsed.label
        }
      } catch { }
      if (!blockLabel && lastNotification.block_name) {
        blockLabel = lastNotification.block_name
      }
    }
    console.info('[currentWatchNotificationsAtom]', {
      previousCount: previous.length,
      nextCount: next.length,
      latest:
        lastNotification && {
          blockName: blockLabel,
          variable: lastNotification.variable_name,
          channel: lastNotification.channel_name,
          isStream: lastNotification.is_stream,
          value: lastNotification.value,
        },
    })

    set(currentWatchNotificationsBaseAtom, next)
  },
)

const highlightedBlocksBaseAtom = atom<Set<string>>(new Set<string>())
export const highlightedBlocksAtom = atom(
  (get) => get(highlightedBlocksBaseAtom),
  (get, set, update: string | Set<string> | ((prev: Set<string>) => Set<string>)) => {
    const prev = get(highlightedBlocksBaseAtom)
    let next: Set<string>

    if (update instanceof Set) {
      next = new Set<string>(update)
    } else if (typeof update === 'function') {
      next = update(prev)
    } else {
      next = new Set(prev)
      next.add(update)
    }

    set(highlightedBlocksBaseAtom, next)
  },
)

// Derived atom for categorized notifications
export const categorizedNotificationsAtom = atom<CategorizedNotifications>((get) => {
  const notifications = get(currentWatchNotificationsAtom)

  const isBlock = (notification: WatchNotification) => {
    try {
      const parsed = JSON.parse(notification.value) as { type?: string } | undefined
      if (parsed?.type === 'block') return true
    } catch { }
    return notification.value.startsWith('Block(')
  }

  const isStream = (notification: WatchNotification) => {
    if (notification.is_stream) return true
    try {
      const parsed = JSON.parse(notification.value) as { type?: string } | undefined
      return typeof parsed?.type === 'string' && parsed.type.startsWith('stream')
    } catch {
      return false
    }
  }

  return {
    variables: notifications.filter(n => n.variable_name && !n.is_stream),
    blocks: notifications.filter(isBlock),
    streams: notifications.filter(isStream),
  }
})
