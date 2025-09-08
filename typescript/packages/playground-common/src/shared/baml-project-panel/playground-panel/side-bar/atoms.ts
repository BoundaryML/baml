import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { vscode } from '../../vscode';
import { runtimeStateAtom } from '../atoms';
import { sessionStore } from '../../../../baml_wasm_web/JotaiProvider';

const getIsEmbed = () => {
  if (typeof window === 'undefined') return false;
  return window.location.href.includes('embed');
};

export const functionsAtom = atom((get) => {
  const runtimeState = get(runtimeStateAtom);
  if (runtimeState === undefined) {
    return [];
  }
  return runtimeState.functions.map((f) => ({
    name: f.name,
    tests: f.test_cases.map((t) => t.name),
  }));
});

export const functionsAreStaleAtom = atom((get) => {
  const runtimeState = get(runtimeStateAtom);
  return runtimeState.stale;
});

export const isSidebarOpenAtom = atomWithStorage(
  'isSidebarOpen',
  getIsEmbed() ? false : vscode.isVscode() ? true : false,
  sessionStore,
);
