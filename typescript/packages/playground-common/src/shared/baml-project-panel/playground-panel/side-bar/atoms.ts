import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { vscode } from '../../vscode';
import { functionsAtom as sdkFunctionsAtom, runtimeAtom } from '../../../../sdk/atoms/core.atoms';
import { sessionStore } from '../../../../baml_wasm_web/JotaiProvider';

const getIsEmbed = () => {
  if (typeof window === 'undefined') return false;
  return window.location.href.includes('embed');
};

export const functionsAtom = atom((get) => {
  const functions = get(sdkFunctionsAtom);
  return functions.map((f) => ({
    name: f.name,
    tests: f.testCases?.map((t) => t.name) || [],
    functionFlavor: f.functionFlavor,
  }));
});

export const functionsAreStaleAtom = atom((get) => {
  const runtime = get(runtimeAtom);
  // Runtime is stale if there's no current runtime but there is a last valid one
  return !runtime.rt && !!runtime.lastValidRt;
});

export const isSidebarOpenAtom = atomWithStorage(
  'isSidebarOpen',
  getIsEmbed() ? false : vscode.isVscode() ? true : false,
  sessionStore,
);
