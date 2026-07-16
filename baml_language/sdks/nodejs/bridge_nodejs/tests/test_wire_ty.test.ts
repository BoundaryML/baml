// Returned generic metadata must preserve every BAML type that has a stable
// runtime spelling in Node. These are the Node mirror of
// sdks/python/tests/test_proto_generics.py's `_ty_to_python_type` cases.

import {
    BamlAudio,
    BamlImage,
    BamlPdf,
    BamlTypeMap,
    BamlVideo,
    Never,
    getTypeMap,
    lowerTypeToWireTy,
    setTypeMap,
} from '../dist/index.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { outboundTyToBamlType } from '../dist/wire_ty.js';

class Box {
    constructor(readonly value?: unknown) {}
}

const Status = {
    OPEN: 'OPEN',
    CLOSED: 'CLOSED',
} as const;

const RecursiveInts = {
    typeAlias: 'user.lorem.RecursiveInts',
} as const;

let savedTypeMap: BamlTypeMap;

beforeEach(() => {
    savedTypeMap = getTypeMap();
    setTypeMap(BamlTypeMap.fromLazyEntries({
        classes: { 'user.lorem.Box': () => Box },
        enums: { 'user.lorem.Status': () => Status },
        typeAliases: { 'user.lorem.RecursiveInts': () => RecursiveInts },
    }));
});

afterEach(() => setTypeMap(savedTypeMap));

describe('returned BAML type metadata', () => {
    test('preserves primitives, containers, and generated classes', () => {
        expect(outboundTyToBamlType({ primitive: { kind: 2 } })).toBe('int');
        expect(outboundTyToBamlType({
            list: { item: { primitive: { kind: 1 } } },
        })).toEqual({ list: 'string' });
        expect(outboundTyToBamlType({
            map: {
                key: { primitive: { kind: 1 } },
                value: { optional: { inner: { primitive: { kind: 4 } } } },
            },
        })).toEqual({ map: ['string', { optional: 'bool' }] });

        const wire = {
            classTy: {
                name: 'user.lorem.Box',
                typeArgs: [{ primitive: { kind: 2 } }],
            },
        };
        const token = outboundTyToBamlType(wire);
        expect(token).toEqual({ class: Box, args: ['int'] });
        expect(lowerTypeToWireTy(token)).toEqual(wire);
    });

    test('preserves structural unions instead of widening to undefined', () => {
        const wire = {
            union: {
                options: [
                    { primitive: { kind: 2 } },
                    { primitive: { kind: 1 } },
                ],
            },
        };
        const token = outboundTyToBamlType(wire);
        expect(token).toEqual({ union: ['int', 'string'] });
        expect(lowerTypeToWireTy(token)).toEqual(wire);
    });

    test('preserves generated enum identity', () => {
        const wire = { enum: { name: 'user.lorem.Status' } };
        const token = outboundTyToBamlType(wire);
        expect(token).toEqual({ enum: Status });
        expect(lowerTypeToWireTy(token)).toEqual(wire);
    });

    test('preserves generated type-alias identity', () => {
        const wire = { typeAlias: { name: 'user.lorem.RecursiveInts', typeArgs: [] } };
        const token = outboundTyToBamlType(wire);
        expect(token).toEqual(RecursiveInts);
        expect(lowerTypeToWireTy(token)).toEqual(wire);
    });

    test.each([
        [{ stringValue: 'draft' }, { kind: 'string', value: 'draft' }],
        [{ intValue: 42 }, { kind: 'int', value: 42 }],
        [{ boolValue: true }, { kind: 'bool', value: true }],
        [
            { bigintValue: '123456789012345678901234567890' },
            { kind: 'bigint', value: 123456789012345678901234567890n },
        ],
        [{ floatValue: '3.140' }, { kind: 'float', value: '3.140' }],
    ] as const)('preserves literal metadata %#', (literal, expected) => {
        const token = outboundTyToBamlType({ literal });
        expect(token).toEqual({ literal: expected });
        expect(lowerTypeToWireTy(token)).toEqual({ literal });
    });

    test.each([
        ['9007199254740992', 9007199254740992n],
        ['-9007199254740992', -9007199254740992n],
        ['9223372036854775807', 9223372036854775807n],
        ['-9223372036854775808', -9223372036854775808n],
    ] as const)('preserves wide int64 literal %s as exact int metadata', (raw, expected) => {
        const literal = baml_bridge.cffi.v1.BamlTyLiteral.fromObject({ intValue: raw });
        const token = outboundTyToBamlType({ literal });

        expect(token).toEqual({ literal: { kind: 'int', value: expected } });

        const lowered = baml_bridge.cffi.v1.BamlTy.fromObject(lowerTypeToWireTy(token));
        expect(lowered.literal?.literal).toBe('intValue');
        expect(String(lowered.literal?.intValue)).toBe(raw);
        expect(lowered.literal?.bigintValue).toBeNull();
        expect(outboundTyToBamlType(lowered)).toEqual(token);
    });

    test.each([
        ['9007199254740991', 9007199254740991],
        ['-9007199254740991', -9007199254740991],
    ] as const)('keeps safe int64 literal %s as number metadata', (raw, expected) => {
        const literal = baml_bridge.cffi.v1.BamlTyLiteral.fromObject({ intValue: raw });

        expect(outboundTyToBamlType({ literal })).toEqual({
            literal: { kind: 'int', value: expected },
        });
    });

    test.each([
        9223372036854775808n,
        -9223372036854775809n,
    ])('rejects int literal %s outside signed int64', (value) => {
        expect(lowerTypeToWireTy({ literal: { kind: 'int', value } })).toEqual({
            unknown: {},
        });
    });

    test.each([
        [1, BamlImage],
        [2, BamlAudio],
        [3, BamlVideo],
        [4, BamlPdf],
    ] as const)('preserves media kind %i as its runtime constructor', (kind, ctor) => {
        const wire = { media: { kind } };
        const token = outboundTyToBamlType(wire);
        expect(token).toBe(ctor);
        expect(lowerTypeToWireTy(token)).toEqual(wire);
    });

    test('leaves generic media unbound', () => {
        expect(outboundTyToBamlType({ media: { kind: 5 } })).toBeUndefined();
    });

    test('preserves never and leaves opaque types unbound', () => {
        expect(outboundTyToBamlType({ never: {} })).toBe(Never);
        expect(outboundTyToBamlType({ rustType: {} })).toBeUndefined();
        expect(outboundTyToBamlType({ unknown: {} })).toBeUndefined();
    });
});
