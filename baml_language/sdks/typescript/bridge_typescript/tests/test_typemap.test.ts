// test_typemap.test.ts — coverage for the runtime BamlTypeMap and
// setTypeMap/getTypeMap.

import path from 'node:path';

import {
    BamlTypeMap,
    setTypeMap,
    getTypeMap,
    BamlError,
    BamlAudio,
    BamlImage,
    BamlPdf,
    BamlStream,
    BamlVideo,
} from '../dist/index.js';
import { encodeCallArgs } from '../dist/proto.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

describe('BamlTypeMap', () => {
    test('empty map getClass throws BamlError', () => {
        const m = new BamlTypeMap();
        expect(() => m.getClass('foo.Bar')).toThrow(BamlError);
    });

    test('lazy thunk resolves and memoizes', () => {
        // A thunk closing over an imported builtin resolves regardless of cwd.
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.PathSep': () => path.sep },
            enums: {},
            typeAliases: {},
        });
        const sep = m.getClass('user.PathSep');
        expect(sep).toBe(path.sep);
        // Second lookup hits the cache (same value).
        expect(m.getClass('user.PathSep')).toBe(sep);
    });

    test('thunk returning undefined throws BamlError', () => {
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.Missing': () => (path as Record<string, unknown>).definitelyNotAnExport },
            enums: {},
            typeAliases: {},
        });
        expect(() => m.getClass('user.Missing')).toThrow(BamlError);
    });

    test('setTypeMap / getTypeMap round-trip', () => {
        const m = new BamlTypeMap();
        setTypeMap(m);
        expect(getTypeMap()).toBe(m);
    });

    test('runtime-owned builtin resolvers preserve constructor identity', () => {
        const entries = {
            'baml.media.Image': BamlImage,
            'baml.media.Audio': BamlAudio,
            'baml.media.Video': BamlVideo,
            'baml.media.Pdf': BamlPdf,
            'ai.stream.Stream': BamlStream,
        } as const;
        const m = BamlTypeMap.fromLazyEntries({
            classes: Object.fromEntries(Object.entries(entries).map(([fqn, ctor]) => [fqn, () => ctor])),
            enums: {},
            typeAliases: {},
        });
        for (const [fqn, ctor] of Object.entries(entries)) {
            expect(m.getClass(fqn)).toBe(ctor);
            expect(m.getClass(fqn)).toBe(ctor);
        }
    });

    test('unbound generic instance carries only nominal class identity', () => {
        class Box<T> {
            static readonly $generic = ['T'] as const;
            value: T;
            $types?: { T?: 'int' };

            constructor(init: { value: T; $types?: { T?: 'int' } }) {
                this.value = init.value;
                this.$types = init.$types;
            }
        }

        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { 'user.test.Box': () => Box },
            enums: {},
            typeAliases: {},
        }));

        const unbound = baml_bridge.cffi.v1.CallFunctionArgs.decode(
            encodeCallArgs({ box: new Box({ value: 5 }) }, { callId: 1n }),
        ).kwargs[0].value!;
        expect(unbound.classValue).toBeDefined();
        expect(unbound.valueType?.classTy?.name).toBe('user.test.Box');
        expect(unbound.valueType?.classTy?.typeArgs).toHaveLength(0);

        const bound = baml_bridge.cffi.v1.CallFunctionArgs.decode(
            encodeCallArgs(
                { box: new Box({ value: 5, $types: { T: 'int' } }) },
                { callId: 2n },
            ),
        ).kwargs[0].value!;
        expect(bound.valueType?.classTy?.typeArgs).toHaveLength(1);
    });

    test('non-generic class instance carries nominal identity', () => {
        class Resume {
            constructor(public name: string) {}
        }

        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { 'user.test.Resume': () => Resume },
            enums: {},
            typeAliases: {},
        }));

        const value = baml_bridge.cffi.v1.CallFunctionArgs.decode(
            encodeCallArgs({ resume: new Resume('hopper') }, { callId: 3n }),
        ).kwargs[0].value!;
        expect(value.classValue).toBeDefined();
        expect(value.valueType?.classTy?.name).toBe('user.test.Resume');
        expect(value.valueType?.classTy?.typeArgs).toHaveLength(0);
    });
});
