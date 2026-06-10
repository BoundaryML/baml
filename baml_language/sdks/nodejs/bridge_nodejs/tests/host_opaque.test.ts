// Opaque host-only values bound to generic positions (bridge generics,
// HOST_VALUE_OPAQUE) — TS side.
//
// TypeScript objects are STRUCTURAL by default at the BAML boundary: a
// class instance encodes as an untagged map (data copy, no identity).
// `opaque(v)` is the explicit opt-in that registers the value in the
// host-value table and sends a sealed handle instead — BAML cannot
// introspect it, `==` is host-object identity, and the same-process
// decoder returns the ORIGINAL reference on round-trip.

import { BamlRuntime } from '../dist/native.js';
import { callFunction } from '../dist/index.js';
import { opaque } from '../dist/proto.js';

const GENERICS_BAML = `
function Identity<T>(x: T) -> T {
    x
}

function Eq<T>(a: T, b: T) -> bool {
    a == b
}

function First<T>(items: T[]) -> T {
    items[0]
}

class Wrapper<T> {
    value T
}

function WrapUnwrap<T>(x: T) -> T {
    let w = Wrapper { value: x };
    w.value
}

function WantsString(x: string) -> string {
    x
}
`;

class MyHostOnly {
    constructor(readonly tag: string) {}
}

function makeRuntime(): BamlRuntime {
    return BamlRuntime.initializeRuntime('.', { 'main.baml': GENERICS_BAML });
}

describe('opaque host-only values (bridge generics)', () => {
    test('opaque identity round-trip is the same object', async () => {
        const rt = makeRuntime();
        const obj = new MyHostOnly('a');
        const out = (await callFunction(rt, 'Identity', { x: opaque(obj) })).result();
        expect(out).toBe(obj); // reference identity
        expect(out).toBeInstanceOf(MyHostOnly);
    });

    test('plain class instance stays structural (data copy, no identity)', async () => {
        const rt = makeRuntime();
        const obj = new MyHostOnly('b');
        const out = (await callFunction(rt, 'Identity', { x: obj })).result();
        // Without `opaque()`, TS objects cross as untagged maps: deep-equal
        // data, but NOT the same reference and NOT an instance of the class.
        expect(out).not.toBe(obj);
        expect(out).not.toBeInstanceOf(MyHostOnly);
        expect((out as Record<string, unknown>).tag).toBe('b');
    });

    test('primitives still round-trip through a generic', async () => {
        const rt = makeRuntime();
        expect((await callFunction(rt, 'Identity', { x: 5 })).result()).toBe(5);
        expect((await callFunction(rt, 'Identity', { x: 'hi' })).result()).toBe('hi');
        expect((await callFunction(rt, 'Identity', { x: [1, 2] })).result()).toEqual([1, 2]);
    });

    test('equality inside BAML is host-object identity', async () => {
        const rt = makeRuntime();
        const a = new MyHostOnly('c');
        const b = new MyHostOnly('c');
        const same = (await callFunction(rt, 'Eq', { a: opaque(a), b: opaque(a) })).result();
        expect(same).toBe(true);
        const diff = (await callFunction(rt, 'Eq', { a: opaque(a), b: opaque(b) })).result();
        expect(diff).toBe(false);
    });

    test('opaque values as list elements', async () => {
        const rt = makeRuntime();
        const first = new MyHostOnly('first');
        const second = new MyHostOnly('second');
        const out = (
            await callFunction(rt, 'First', { items: [opaque(first), opaque(second)] })
        ).result();
        expect(out).toBe(first);
    });

    test('opaque value through a generic class field', async () => {
        const rt = makeRuntime();
        const obj = new MyHostOnly('field');
        const out = (await callFunction(rt, 'WrapUnwrap', { x: opaque(obj) })).result();
        expect(out).toBe(obj);
    });

    test('opaque value rejected at a concrete string param', async () => {
        const rt = makeRuntime();
        const obj = new MyHostOnly('nope');
        await expect(
            callFunction(rt, 'WantsString', { x: opaque(obj) }),
        ).rejects.toThrow(/host-only value/);
    });

    test('symbols auto-opaque without a wrapper', async () => {
        const rt = makeRuntime();
        const sym = Symbol('mine');
        const out = (await callFunction(rt, 'Identity', { x: sym })).result();
        expect(out).toBe(sym);
    });
});
