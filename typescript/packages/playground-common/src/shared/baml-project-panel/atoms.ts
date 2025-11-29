/**
 * Backward Compatibility Layer
 *
 * This file re-exports atoms from the SDK to maintain backward compatibility
 * with existing code that imports from this location.
 *
 * All atoms are now managed by the SDK, ensuring consistent state across the app.
 */

import { atom, useAtomValue, useSetAtom } from 'jotai';
import { useEffect } from 'react';
import { vscode } from './vscode';

// Re-export all core atoms from SDK
export {
  // WASM and runtime atoms
  wasmAtom,
  filesAtom,
  runtimeAtom,
  ctxAtom,
  versionAtom,

  // Diagnostics and errors
  diagnosticsAtom,
  numErrorsAtom,
  isRuntimeValid,
  wasmPanicAtom,

  // Generated files
  generatedFilesAtom,
  generatedFilesByLangAtomFamily,

  // Feature flags and settings
  featureFlagsAtom,
  betaFeatureEnabledAtom,
  vscodeSettingsAtom,
  playgroundPortAtom,
  proxyUrlAtom,
  envVarsAtom,

  // File tracking
  bamlFilesTrackedAtom,
  sandboxFilesTrackedAtom,

  // Selection
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  selectedFunctionObjectAtom,
  selectedTestCaseAtom,
  selectionAtom,
  functionTestSnippetAtom,
  unifiedSelectionStateAtom,

  // Functions
  functionsAtom,

  // Types
  type DiagnosticError,
  type GeneratedFile,
  type WasmPanicState,
  type VSCodeSettings,
  type SelectionState,
} from '../../sdk/atoms/core.atoms';

// Re-export test atoms from SDK
export {
  areTestsRunningAtom,
  currentAbortControllerAtom,
  flashRangesAtom,
  testHistoryAtom,
  selectedHistoryIndexAtom,
  selectedTestHistoryAtom,
  currentWatchNotificationsAtom,
  highlightedBlocksAtom,
  categorizedNotificationsAtom,
  type TestState,
  type TestHistoryEntry,
  type TestHistoryRun,
  type WatchNotification,
  type FlashRange,
  type CategorizedNotifications,
} from '../../sdk/atoms/test.atoms';

// ============================================================================
// WASM Panic Handling
// ============================================================================

import { wasmPanicAtom } from '../../sdk/atoms/core.atoms';

// Global setter function that will be wired up by useWasmPanicHandler
let globalSetPanic: ((msg: string) => void) | null = null;

// Set up the global panic handler BEFORE WASM loads
// This must be defined before wasmAtomAsync is evaluated
if (typeof window !== 'undefined') {
  (window as any).__onWasmPanic = (msg: string) => {
    console.error('[WASM Panic]', msg);

    // Call the setter if it's been wired up
    if (globalSetPanic) {
      globalSetPanic(msg);
    } else {
      console.warn('[WASM Panic] Handler called but atom setter not yet initialized');
    }
  };
}

/**
 * Hook to wire up the WASM panic handler to the Jotai atom.
 * Call this once in your root component to enable panic state tracking.
 *
 * @example
 * ```tsx
 * function App() {
 *   useWasmPanicHandler();
 *   return <YourApp />;
 * }
 * ```
 */
export const useWasmPanicHandler = () => {
  const setPanic = useSetAtom(wasmPanicAtom);

  useEffect(() => {
    // Wire up the global setter with telemetry
    globalSetPanic = (msg: string) => {
      const timestamp = Date.now();
      setPanic({ msg, timestamp });

      // Send telemetry about the panic
      vscode.sendTelemetry({
        action: 'wasm_panic',
        data: {
          panic_message: msg,
          timestamp,
          during_test_execution: false, // Will be overridden in test-runner if during test
        },
      });
    };

    // Cleanup on unmount
    return () => {
      globalSetPanic = null;
    };
  }, [setPanic]);
};

/**
 * Hook to clear panic state.
 * Use this to dismiss panic notifications in your UI.
 */
export const useClearWasmPanic = () => {
  const setPanic = useSetAtom(wasmPanicAtom);
  return () => setPanic(null);
};

// ============================================================================
// Legacy Aliases (for backward compatibility)
// ============================================================================

// Alias sandboxFilesAtom to sandboxFilesTrackedAtom if needed
import { sandboxFilesTrackedAtom } from '../../sdk/atoms/core.atoms';
export { sandboxFilesTrackedAtom as sandboxFilesAtom };
