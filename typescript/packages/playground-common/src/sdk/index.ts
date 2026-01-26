/**
 * BAML SDK - Refactored Architecture
 *
 * Follows the immutable runtime pattern:
 * - Runtime is recreated on file changes (like wasmAtom)
 * - SDK orchestrates runtime and storage
 * - Storage abstraction allows swapping state management
 */

// Re-export BAMLSDK class from separate file to avoid circular dependencies
export { BAMLSDK } from './sdk';

// Re-export types
export * from './types';
export type {
  BamlRuntimeInterface,
  BamlRuntimeFactory,
  ExecutionOptions,
} from './runtime/BamlRuntimeInterface';
export * from './storage/SDKStorage';
export * from './mock-config/types';
export type {
  TestState,
  TestHistoryEntry,
  TestHistoryRun,
  FlashRange,
  CategorizedNotifications,
} from './atoms/test.atoms';

// Re-export hooks and provider
export * from './hooks';
export * from './provider';
export { BAMLSDKContext } from './context';

// Re-export debug fixtures for testing
export { DEBUG_BAML_FILES } from './debugFixtures';
// Factory functions are available via direct import from './factory' to avoid circular dependencies
