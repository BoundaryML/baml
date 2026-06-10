// Explicit generic type arguments ($types / BEP-039) — TS side.
//
// `callFunction(rt, name, kwargs, ..., typeArgs)` carries structural type
// tokens over the wire as `CallFunctionArgs.type_args`; the engine
// substitutes them into the signature template so TypeVar positions behave
// exactly like concrete declarations.

import { BamlRuntime } from '../dist/native.js';
import { callFunction } from '../dist/index.js';

const TYPE_ARGS_BAML = `
function TypeNameOf<T>() -> string {
    reflect.type_of<T>().to_string()
}

function ParseAs<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    baml.json.from_string<T>(s)
}

function Identity<T>(x: T) -> T {
    x
}

class Profile {
    name string
    age int
}
`;

function makeRuntime(): BamlRuntime {
    return BamlRuntime.initializeRuntime('.', { 'main.baml': TYPE_ARGS_BAML });
}

describe('$types explicit type arguments', () => {
    test('reflect sees the explicit binding', async () => {
        const rt = makeRuntime();
        const out = (
            await callFunction(rt, 'TypeNameOf', {}, undefined, undefined, undefined, ['string'])
        ).result();
        expect(out).toBe('string');
    });

    test('unbound defaults to unknown', async () => {
        const rt = makeRuntime();
        const out = (await callFunction(rt, 'TypeNameOf', {})).result();
        expect(out).toBe('unknown');
    });

    test('ParseAs<int> binds the JSON parse target', async () => {
        const rt = makeRuntime();
        const out = (
            await callFunction(rt, 'ParseAs', { s: '42' }, undefined, undefined, undefined, ['int'])
        ).result();
        expect(out).toBe(42);
    });

    test('ParseAs<Profile> parses into the class', async () => {
        const rt = makeRuntime();
        const out = (
            await callFunction(
                rt,
                'ParseAs',
                { s: '{"name": "ada", "age": 36}' },
                undefined,
                undefined,
                undefined,
                ['Profile'],
            )
        ).result() as Record<string, unknown> & { name?: string };
        const name = (out as { name?: unknown }).name ?? (out as Record<string, unknown>)['name'];
        expect(name).toBe('ada');
    });

    test('structural token: list of int', async () => {
        const rt = makeRuntime();
        const out = (
            await callFunction(rt, 'ParseAs', { s: '[1, 2, 3]' }, undefined, undefined, undefined, [
                { list: 'int' },
            ])
        ).result();
        expect(out).toEqual([1, 2, 3]);
    });

    test('unknown type name surfaces an error', async () => {
        const rt = makeRuntime();
        await expect(
            callFunction(rt, 'TypeNameOf', {}, undefined, undefined, undefined, ['NoSuchType']),
        ).rejects.toThrow(/unknown type/);
    });
});
