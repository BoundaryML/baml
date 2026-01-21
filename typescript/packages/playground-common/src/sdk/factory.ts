/**
 * SDK Factory Functions
 *
 * Convenience functions for creating SDK instances with different configurations
 */

import type { createStore } from 'jotai';
import { BAMLSDK } from './sdk';
import { JotaiStorage } from './storage/JotaiStorage';
import { MockBamlRuntime } from './runtime/MockBamlRuntime';
import { BamlRuntime } from './runtime/BamlRuntime';
import { createMockRuntimeConfig } from './mock-config/config';

/**
 * Create SDK with mock runtime
 */
export function createMockSDK(
  store: ReturnType<typeof createStore>,
  options?: {
    cacheHitRate?: number;
    errorRate?: number;
    verboseLogging?: boolean;
    speedMultiplier?: number;
  }
): BAMLSDK {
  console.log('[createMockSDK] Creating mock SDK with options:', options);
  // Create mock configuration
  const mockConfig = createMockRuntimeConfig(options);

  // Create runtime factory (accepts env vars and feature flags but ignores them for mock)
  const runtimeFactory = async (
    files: Record<string, string>,
    _envVars?: Record<string, string>,
    _featureFlags?: string[]
  ) => {
    return await MockBamlRuntime.create(files, mockConfig);
  };

  // Create storage
  const storage = new JotaiStorage(store);

  // Create SDK
  return new BAMLSDK(runtimeFactory, storage);
}

/**
 * Create fast mock SDK for testing (no delays)
 */
export function createFastMockSDK(store: ReturnType<typeof createStore>): BAMLSDK {
  return createMockSDK(store, {
    speedMultiplier: 0.1,
    verboseLogging: false,
    cacheHitRate: 0,
    errorRate: 0,
  });
}

/**
 * Create error-prone mock SDK for testing error handling
 */
export function createErrorProneSDK(store: ReturnType<typeof createStore>): BAMLSDK {
  return createMockSDK(store, {
    speedMultiplier: 1,
    verboseLogging: true,
    cacheHitRate: 0,
    errorRate: 0.5,
  });
}

/**
 * Create SDK with real BAML runtime (WASM)
 *
 * This creates a production SDK that uses the actual BAML compiler.
 */
export function createRealBAMLSDK(store: ReturnType<typeof createStore>): BAMLSDK {
  // Create storage
  const storage = new JotaiStorage(store);
  // Create runtime factory that uses real BAML runtime
  const runtimeFactory = async (
    files: Record<string, string>,
    envVars?: Record<string, string>,
    featureFlags?: string[]
  ) => {
    console.log('[createRealBAMLSDK] Creating runtime with files', files);
    const { wasm, runtime } = await BamlRuntime.create(files, envVars || {}, featureFlags || []);
    storage.setWasm(wasm);
    return runtime;
  };



  // Create SDK
  return new BAMLSDK(runtimeFactory, storage);
}
