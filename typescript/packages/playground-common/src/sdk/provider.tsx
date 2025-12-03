/**
 * BAML SDK Provider for React - Refactored
 *
 * Provides the SDK instance through React Context
 * Uses the new runtime factory pattern
 */

import { Provider as JotaiProvider, createStore } from 'jotai';
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { createMockSDK, createRealBAMLSDK } from './factory';
import {
  DebugBanner,
  isDebugMode,
  getPersistedRuntimeMode,
  persistRuntimeMode,
  type RuntimeMode,
} from './DebugBanner';
import { DEBUG_BAML_FILES } from './debugFixtures';
import { BAMLSDKContext } from './context';

interface BAMLSDKProviderProps {
  children: ReactNode;
  mode?: RuntimeMode;
}

/**
 * Provider component that wraps the app and provides SDK access
 */
export function BAMLSDKProvider({ children, mode: initialMode = 'wasm' }: BAMLSDKProviderProps) {
  // Check if debug mode is enabled (memoized to avoid SSR issues)
  const debugMode = useMemo(() => isDebugMode(), []);

  // Get persisted runtime mode or use initial mode (lazy initialization)
  const [runtimeMode, setRuntimeMode] = useState<RuntimeMode>(() => {
    return debugMode ? (getPersistedRuntimeMode() || initialMode) : initialMode;
  });

  const [isInitialized, setIsInitialized] = useState(false);

  // Create store once and never recreate it
  const store = useMemo(() => createStore(), []);

  // Create SDK whenever mode changes
  const sdk = useMemo(() => {
    console.log('[BAMLSDKProvider] Creating SDK with mode:', runtimeMode, store);
    console.log('🚀 Creating BAML SDK with mode:', runtimeMode);
    if (runtimeMode === 'mock') {
      return createMockSDK(store);
    } else if (runtimeMode === 'wasm') {
      return createRealBAMLSDK(store);
    }
    throw new Error(`Unsupported mode: ${runtimeMode}`);
  }, [runtimeMode, store]);

  // Handle mode change from debug banner
  const handleModeChange = useCallback(
    (newMode: RuntimeMode) => {
      if (newMode === runtimeMode) return;

      console.log('🔄 Switching runtime mode:', runtimeMode, '->', newMode);
      persistRuntimeMode(newMode);
      setIsInitialized(false);
      setRuntimeMode(newMode);
    },
    [runtimeMode]
  );

  // Handle async initialization - reinitialize when SDK changes
  useEffect(() => {
    let mounted = true;

    async function init() {
      console.log('⏳ Initializing SDK with mode:', runtimeMode);

      // Use debug fixtures when in debug mode, otherwise use empty files
      const initialFiles = debugMode
        ? DEBUG_BAML_FILES
        : {
          'workflows/simple.baml': '// Mock workflow file',
          'workflows/conditional.baml': '// Mock conditional workflow',
        };

      await sdk.initialize(initialFiles);

      if (mounted) {
        console.log('✅ SDK initialized successfully');
        const workflows = sdk.workflows.getAll();
        console.log('📦 Loaded workflows:', workflows.length, workflows.map((w) => w.id));
        setIsInitialized(true);
      }
    }

    init();

    return () => {
      mounted = false;
    };
  }, [sdk, runtimeMode, debugMode]);

  return (
    <BAMLSDKContext.Provider value={sdk}>
      <JotaiProvider store={store}>
        {debugMode && <DebugBanner currentMode={runtimeMode} onModeChange={handleModeChange} />}
        {!isInitialized ? (
          <div className="w-screen h-screen flex items-center justify-center bg-background">
            <div className="text-center">
              <div className="text-xl font-semibold">Loading BAML SDK...</div>
              <div className="text-sm text-muted-foreground mt-2">
                Initializing {runtimeMode === 'mock' ? 'mock' : 'WASM'} runtime
              </div>
            </div>
          </div>
        ) : (
          children
        )}
      </JotaiProvider>
    </BAMLSDKContext.Provider>
  );
}

// Re-export RuntimeMode type and debug utilities for convenience
export type { RuntimeMode } from './DebugBanner';
export { isDebugMode } from './DebugBanner';
