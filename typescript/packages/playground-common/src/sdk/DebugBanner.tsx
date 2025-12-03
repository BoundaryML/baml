/**
 * Debug Banner Component
 *
 * Shows a banner at the top in debug mode that allows toggling between
 * mock runtime and real WASM runtime
 *
 * To enable debug mode:
 * 1. Add ?debug=true to the URL (e.g., http://localhost:3030?debug=true)
 * 2. Or set localStorage.debug = 'true' in browser console
 *
 * The selected runtime mode is persisted in localStorage so it remains
 * active across page reloads when debug mode is enabled.
 */

import { useState } from 'react';

export type RuntimeMode = 'mock' | 'wasm';

interface DebugBannerProps {
  currentMode: RuntimeMode;
  onModeChange: (mode: RuntimeMode) => void;
}

export function DebugBanner({ currentMode, onModeChange }: DebugBannerProps) {
  return (
    <div className="bg-yellow-500 text-black px-4 py-2 flex items-center justify-between">
      <div className="flex items-center gap-4">
        <span className="font-semibold">🔧 DEBUG MODE</span>
        <span className="text-sm">Runtime:</span>
        <div className="flex gap-2">
          <button
            onClick={() => onModeChange('mock')}
            className={`px-3 py-1 rounded text-sm font-medium transition-colors ${currentMode === 'mock'
              ? 'bg-black text-yellow-500'
              : 'bg-yellow-600 hover:bg-yellow-700 text-white'
              }`}
          >
            Mock Runtime
          </button>
          <button
            onClick={() => onModeChange('wasm')}
            className={`px-3 py-1 rounded text-sm font-medium transition-colors ${currentMode === 'wasm'
              ? 'bg-black text-yellow-500'
              : 'bg-yellow-600 hover:bg-yellow-700 text-white'
              }`}
          >
            WASM Runtime
          </button>
        </div>
      </div>
      <div className="text-xs text-yellow-900">
        Set <code>?debug=true</code> or localStorage.debug=true to enable
      </div>
    </div>
  );
}

/**
 * Check if debug mode is enabled
 * Checks URL params and localStorage
 */
export function isDebugMode(): boolean {
  // Check if we're in a browser environment
  if (typeof window === 'undefined') {
    return false;
  }

  // Check URL params
  try {
    const urlParams = new URLSearchParams(window.location.search);
    if (urlParams.get('debug') === 'true') {
      return true;
    }
  } catch (e) {
    // window.location may not be available
  }

  // Check localStorage
  try {
    if (localStorage.getItem('debug') === 'true') {
      return true;
    }
  } catch (e) {
    // localStorage may not be available
  }

  return false;
}

/**
 * Get the runtime mode from localStorage (persisted selection)
 */
export function getPersistedRuntimeMode(): RuntimeMode | null {
  // Check if we're in a browser environment
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    const mode = localStorage.getItem('baml_runtime_mode');
    if (mode === 'mock' || mode === 'wasm') {
      console.log('🔧 Getting persisted runtime mode:', mode);
      return mode;
    }
  } catch (e) {
    // localStorage may not be available
  }
  return null;
}

/**
 * Persist the runtime mode to localStorage
 */
export function persistRuntimeMode(mode: RuntimeMode): void {
  // Check if we're in a browser environment
  if (typeof window === 'undefined') {
    return;
  }

  try {
    localStorage.setItem('baml_runtime_mode', mode);
  } catch (e) {
    // localStorage may not be available
  }
}
