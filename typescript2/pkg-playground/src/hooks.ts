'use client';

import { useContext } from 'react';
import { useSetAtom } from 'jotai';
import { filesAtom } from './atoms';
import { BamlContext } from './context';

function useBamlContext() {
  const context = useContext(BamlContext);
  if (!context) {
    throw new Error('useBamlContext must be used within a BamlPlaygroundProvider');
  }
  return context;
}

/**
 * Hook to access WASM ready state.
 */
export function useWasmReady() {
  return useBamlContext().isReady;
}

/**
 * Hook to access WASM error state.
 */
export function useWasmError() {
  return useBamlContext().error;
}

/**
 * Hook to access the list of functions.
 * Calls list_functions() directly - Salsa handles memoization in WASM.
 */
export function useFunctions() {
  const { projectRef } = useBamlContext();
  return projectRef.current?.list_functions() ?? [];
}

/**
 * Hook to update a file.
 */
export function useUpdateFile() {
  const { projectRef } = useBamlContext();
  const setFiles = useSetAtom(filesAtom);

  return (path: string, content: string) => {
    if (!projectRef.current) return;
    projectRef.current.update_file(path, content);
    setFiles((prev) => ({ ...prev, [path]: content }));
  };
}

/**
 * Convenience hook that combines all playground state.
 * For more granular updates, use the individual hooks.
 */
export function useBamlPlayground() {
  const { projectRef, isReady, error } = useBamlContext();
  const setFiles = useSetAtom(filesAtom);

  const functions = projectRef.current?.list_functions() ?? [];

  const updateFile = (path: string, content: string) => {
    if (!projectRef.current) return;
    projectRef.current.update_file(path, content);
    setFiles((prev) => ({ ...prev, [path]: content }));
  };

  return { isReady, error, functions, updateFile };
}
