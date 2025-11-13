/**
 * BAML SDK Provider for React - Refactored
 *
 * Provides the SDK instance through React Context
 * Uses the new runtime factory pattern
 */

import { Provider as JotaiProvider, createStore } from 'jotai';
import { createContext, useEffect, useRef, useState, type ReactNode } from 'react';
import type { BAMLSDK } from './sdk';
import { createMockSDK, createRealBAMLSDK } from './factory';
import {
  DebugBanner,
  isDebugMode,
  getPersistedRuntimeMode,
  persistRuntimeMode,
  type RuntimeMode,
} from './DebugBanner';
import { DEBUG_BAML_FILES } from './debugFixtures';

// Export context for use in hooks
export const BAMLSDKContext = createContext<BAMLSDK | null>(null);

interface BAMLSDKProviderProps {
  children: ReactNode;
  mode?: RuntimeMode;
}

/**
 * Provider component that wraps the app and provides SDK access
 */
export function BAMLSDKProvider({ children, mode: initialMode = 'mock' }: BAMLSDKProviderProps) {
  // Check if debug mode is enabled (lazy initialization to avoid SSR issues)
  const [debugMode] = useState(() => isDebugMode());

  // Get persisted runtime mode or use initial mode (lazy initialization)
  const [runtimeMode, setRuntimeMode] = useState<RuntimeMode>(() => {
    const defaultMode = debugMode ? (getPersistedRuntimeMode() || initialMode) : initialMode;
    return defaultMode;
  });

  // Create refs to ensure single instance creation
  const storeRef = useRef<ReturnType<typeof createStore> | undefined>(undefined);
  const sdkRef = useRef<BAMLSDK | undefined>(undefined);

  // Track which mode the current SDK was created with
  const currentSDKModeRef = useRef<RuntimeMode | undefined>(undefined);

  // Initialize store once
  if (!storeRef.current) {
    storeRef.current = createStore();
  }

  // Create or recreate SDK when mode changes
  if (!sdkRef.current || currentSDKModeRef.current !== runtimeMode) {
    console.log('🚀 Creating BAML SDK with mode: ', runtimeMode);
    if (runtimeMode === 'mock') {
      sdkRef.current = createMockSDK(storeRef.current);
    } else if (runtimeMode === 'wasm') {
      sdkRef.current = createRealBAMLSDK(storeRef.current);
    } else {
      throw new Error(`Unsupported mode: ${runtimeMode}`);
    }
    currentSDKModeRef.current = runtimeMode;
  }

  const [isInitialized, setIsInitialized] = useState(false);

  // Handle mode change from debug banner
  const handleModeChange = (newMode: RuntimeMode) => {
    if (newMode === runtimeMode) return;

    console.log('🔄 Switching runtime mode:', runtimeMode, '->', newMode);

    // Persist the new mode
    persistRuntimeMode(newMode);

    // Reset initialization state
    setIsInitialized(false);

    // Update mode (this will trigger SDK recreation)
    setRuntimeMode(newMode);
  };

  // Handle async initialization - reinitialize when runtime mode changes
  useEffect(() => {
    let mounted = true;

    async function init() {
      if (!sdkRef.current) return;

      console.log('⏳ Initializing SDK with mode:', runtimeMode);

      // Use debug fixtures when in debug mode, otherwise use empty files
      const initialFiles = debugMode
        ? DEBUG_BAML_FILES
        : {
          'workflows/simple.baml': '// Mock workflow file',
          'workflows/conditional.baml': '// Mock conditional workflow',
        };

      await sdkRef.current.initialize(initialFiles);

      if (mounted) {
        console.log('✅ SDK initialized successfully');
        const workflows = sdkRef.current.workflows.getAll();
        console.log('📦 Loaded workflows:', workflows.length, workflows.map((w) => w.id));
        setIsInitialized(true);
      }
    }

    init();

    return () => {
      mounted = false;
    };
  }, [runtimeMode, debugMode]); // Reinitialize when runtime mode changes

  return (
    <BAMLSDKContext.Provider value={sdkRef.current}>
      <JotaiProvider store={storeRef.current}>
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
