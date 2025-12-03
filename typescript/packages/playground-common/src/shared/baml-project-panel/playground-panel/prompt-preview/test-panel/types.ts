/**
 * Test Panel Types
 *
 * Re-exports types from SDK and provides UI-specific types.
 */

import type { WatchNotification as SDKWatchNotification } from '../../../../../sdk/atoms/test.atoms';

// Re-export core types from SDK
export type {
  WatchNotification,
  CategorizedNotifications,
} from '../../../../../sdk/atoms/test.atoms';

// UI-specific types
export type WatchHandler = (notification: SDKWatchNotification) => void;
