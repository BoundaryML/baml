// test_engine.test.ts — mirrors bridge_python/tests/test_engine.py

import { BamlRuntime, callFunctionSync, callFunction,
         AbortController, getVersion, flushEvents } from '../index';

const BAML_SOURCE = `
function ReturnOne() -> int {
  1
}

function CallReturnOne() -> int {
  ReturnOne()
}

function ChainedCalls() -> int {
  CallReturnOne()
}

function AddNumbers(a: int, b: int) -> int {
  a + b
}

enum Direction {
  North,
  South,
  East,
  West,
}

function Heading(goNorth: bool) -> Direction {
  if (goNorth) { Direction.North } else { Direction.South }
}

class Rect {
  width int
  height int
}

function MakeRect(w: int, h: int) -> Rect {
  Rect { width: w, height: h }
}

function RectOrDirection(useRect: bool) -> Rect | Direction {
  if (useRect) { Rect { width: 5, height: 10 } } else { Direction.East }
}
`;

function makeRuntime(bamlSource: string): BamlRuntime {
    return BamlRuntime.fromFiles('.', { 'main.baml': bamlSource });
}

describe('Basics', () => {
    test('get_version returns non-empty string', () => {
        const v = getVersion();
        expect(typeof v).toBe('string');
        expect(v.length).toBeGreaterThan(0);
    });

    test('from_files creates runtime', () => {
        const rt = makeRuntime(BAML_SOURCE);
        expect(rt).toBeDefined();
    });

    test('flush_events runs without error', () => {
        expect(() => flushEvents()).not.toThrow();
    });
});

describe('callFunctionSync', () => {
    const rt = makeRuntime(BAML_SOURCE);

    test('ReturnOne', () => {
        expect(callFunctionSync(rt, 'ReturnOne', {}).result()).toBe(1);
    });

    test('AddNumbers', () => {
        expect(callFunctionSync(rt, 'AddNumbers', { a: 3, b: 4 }).result()).toBe(7);
    });

    test('enum result — Heading(true) → North', () => {
        expect(callFunctionSync(rt, 'Heading', { goNorth: true }).result()).toBe('North');
    });

    test('enum result — Heading(false) → South', () => {
        expect(callFunctionSync(rt, 'Heading', { goNorth: false }).result()).toBe('South');
    });

    test('class result — MakeRect(10, 20)', () => {
        expect(callFunctionSync(rt, 'MakeRect', { w: 10, h: 20 }).result()).toEqual({
            width: 10,
            height: 20,
        });
    });

    test('union result — class branch', () => {
        expect(callFunctionSync(rt, 'RectOrDirection', { useRect: true }).result()).toEqual({
            width: 5,
            height: 10,
        });
    });

    test('union result — enum branch', () => {
        expect(callFunctionSync(rt, 'RectOrDirection', { useRect: false }).result()).toBe('East');
    });

    test('function not found', () => {
        expect(() => callFunctionSync(rt, 'NonExistent', {})).toThrow();
    });
});

describe('callFunction (async)', () => {
    const rt = makeRuntime(BAML_SOURCE);

    test('ReturnOne', async () => {
        expect((await callFunction(rt, 'ReturnOne', {})).result()).toBe(1);
    });

    test('AddNumbers', async () => {
        expect((await callFunction(rt, 'AddNumbers', { a: 10, b: 20 })).result()).toBe(30);
    });

    test('ChainedCalls', async () => {
        expect((await callFunction(rt, 'ChainedCalls', {})).result()).toBe(1);
    });

    test('async enum result — Heading', async () => {
        expect((await callFunction(rt, 'Heading', { goNorth: true })).result()).toBe('North');
    });

    test('async class result — MakeRect', async () => {
        expect((await callFunction(rt, 'MakeRect', { w: 3, h: 7 })).result()).toEqual({
            width: 3,
            height: 7,
        });
    });

    test('async union result — RectOrDirection', async () => {
        const classResult = await callFunction(rt, 'RectOrDirection', { useRect: true });
        expect(classResult.result()).toEqual({ width: 5, height: 10 });
        const enumResult = await callFunction(rt, 'RectOrDirection', { useRect: false });
        expect(enumResult.result()).toBe('East');
    });
});

describe('AbortController', () => {
    test('starts not aborted', () => {
        const ac = new AbortController();
        expect(ac.aborted).toBe(false);
    });

    test('abort sets aborted', () => {
        const ac = new AbortController();
        ac.abort();
        expect(ac.aborted).toBe(true);
    });
});
