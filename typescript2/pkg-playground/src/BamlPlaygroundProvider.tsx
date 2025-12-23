'use client';

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useSetAtom } from 'jotai';
import initWasm, { WasmProject } from 'baml-playground-wasm';
import { filesAtom } from './atoms';
import { BamlContext } from './context';

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
 * Uses useRef for the project instance to avoid unnecessary re-renders.
 */
export function BamlPlaygroundProvider({
  initialFiles = {},
  rootDir = 'baml_src',
  children,
}: BamlPlaygroundProviderProps) {
  const projectRef = useRef<WasmProject | null>(null);
  const initialFilesRef = useRef(initialFiles);
  const [isReady, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setFiles = useSetAtom(filesAtom);

  useEffect(() => {
    let cancelled = false;

    async function init() {
      try {
        await initWasm();
        if (cancelled) return;

        const project = new WasmProject(rootDir, initialFilesRef.current);
        projectRef.current = project;
        setFiles(initialFilesRef.current);
        setReady(true);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      }
    }

    init();

    return () => {
      cancelled = true;
      projectRef.current?.free();
      projectRef.current = null;
    };
  }, []);

  return (
    <BamlContext.Provider value={{ projectRef, isReady, error }}>
      {children}
    </BamlContext.Provider>
  );
}
