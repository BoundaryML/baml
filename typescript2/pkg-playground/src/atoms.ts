'use client';

import { atom } from 'jotai';
import type { WasmProject } from 'baml-playground-wasm';

/** Whether the WASM module has been initialized */
export const wasmReadyAtom = atom(false);

/** Error message if WASM initialization failed */
export const wasmErrorAtom = atom<string | null>(null);

/** The current WASM project instance */
export const projectAtom = atom<WasmProject | null>(null);

/** List of function names in the project */
export const functionsAtom = atom<string[]>([]);

/** Currently selected function */
export const selectedFunctionAtom = atom<string | undefined>(undefined);

/** File contents map (path -> content) */
export const filesAtom = atom<Record<string, string>>({});
