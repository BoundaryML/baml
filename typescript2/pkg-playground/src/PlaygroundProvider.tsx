'use client';

import { useMemo } from 'react';
import { atom, useAtom } from 'jotai';

type PlaygroundState = {
  code: string;
  setCode: (value: string) => void;
};

export const codeAtom = atom<string>('');

export const usePlayground = (): PlaygroundState => {
  const [code, setCode] = useAtom(codeAtom);
  return useMemo<PlaygroundState>(() => ({ code, setCode }), [code, setCode]);
};
