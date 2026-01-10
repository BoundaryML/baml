'use client';

import { useMemo } from 'react';
import { atom, useAtom } from 'jotai';

type PlaygroundState = {
  code: string;
  setCode: (value: string) => void;
};

const DEFAULT_BAML_CODE = `function assertOk() -> int {

    assert 2 + 2 == 4;

    3
}

function assertNotOk() -> int {
    assert 3 == 1;

    2
}

// should yield error
function assertBool() -> int {

    assert "string";

    1
}
`;

export const codeAtom = atom<string>(DEFAULT_BAML_CODE);

export const usePlayground = (): PlaygroundState => {
  const [code, setCode] = useAtom(codeAtom);
  return useMemo<PlaygroundState>(() => ({ code, setCode }), [code, setCode]);
};
