'use client';

import { atom } from 'jotai';

/** Currently selected function */
export const selectedFunctionAtom = atom<string | undefined>(undefined);

/** File contents map (path -> content) */
export const filesAtom = atom<Record<string, string>>({});
