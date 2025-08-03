import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { vscode } from '../../vscode';
import { runtimeStateAtom } from '../atoms';

const isEmbed =
  typeof window !== 'undefined' && window.location.href.includes('embed');

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
  isEmbed ? false : vscode.isVscode() ? true : false,
);
