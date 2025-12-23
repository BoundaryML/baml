'use client';

import { useEffect, type ReactNode } from 'react';
import { useSetAtom, useAtomValue } from 'jotai';
import initWasm, { WasmProject } from 'baml-playground-wasm';
import {
  wasmReadyAtom,
  wasmErrorAtom,
  projectAtom,
  functionsAtom,
  filesAtom,
} from './atoms';

interface BamlPlaygroundProviderProps {
  /** Initial files to load (path -> content) */
  initialFiles?: Record<string, string>;
  /** Root directory name */
  rootDir?: string;
  /** Children to render */
  children: ReactNode;
}

/**
 * Provider component that initializes the BAML WASM module.
 * Uses jotai atoms for state management - no context needed.
 */
export function BamlPlaygroundProvider({
  initialFiles = {},
  rootDir = 'baml_src',
  children,
}: BamlPlaygroundProviderProps) {
  const setReady = useSetAtom(wasmReadyAtom);
  const setError = useSetAtom(wasmErrorAtom);
  const setProject = useSetAtom(projectAtom);
  const setFunctions = useSetAtom(functionsAtom);
  const setFiles = useSetAtom(filesAtom);

  useEffect(() => {
    let cancelled = false;
    let currentProject: WasmProject | null = null;

    async function init() {
      try {
        await initWasm();
        if (cancelled) return;

        currentProject = new WasmProject(rootDir, initialFiles);
        setProject(currentProject);
        setFunctions(currentProject.list_functions());
        setFiles(initialFiles);
        setReady(true);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    }

    init();

    return () => {
      cancelled = true;
      currentProject?.free();
    };
  }, []);

  return <>{children}</>;
}

/**
 * Hook to access WASM ready state.
 */
export function useWasmReady() {
  return useAtomValue(wasmReadyAtom);
}

/**
 * Hook to access WASM error state.
 */
export function useWasmError() {
  return useAtomValue(wasmErrorAtom);
}

/**
 * Hook to access the list of functions.
 */
export function useFunctions() {
  return useAtomValue(functionsAtom);
}

/**
 * Hook to update a file and refresh the function list.
 */
export function useUpdateFile() {
  const project = useAtomValue(projectAtom);
  const setFunctions = useSetAtom(functionsAtom);
  const setFiles = useSetAtom(filesAtom);

  return (path: string, content: string) => {
    if (!project) return;
    project.update_file(path, content);
    setFunctions(project.list_functions());
    setFiles((prev) => ({ ...prev, [path]: content }));
  };
}

/**
 * Hook to get and set the selected function.
 */
export { selectedFunctionAtom } from './atoms';

/**
 * Convenience hook that combines all playground state.
 * For more granular updates, use the individual hooks.
 */
export function useBamlPlayground() {
  const isReady = useWasmReady();
  const error = useWasmError();
  const functions = useFunctions();
  const updateFile = useUpdateFile();

  return { isReady, error, functions, updateFile };
}
