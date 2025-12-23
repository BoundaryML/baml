'use client';

import { createContext, type MutableRefObject } from 'react';
import type { WasmProject } from 'baml-playground-wasm';

export interface BamlContextValue {
  projectRef: MutableRefObject<WasmProject | null>;
  isReady: boolean;
  error: string | null;
}

export const BamlContext = createContext<BamlContextValue | null>(null);
