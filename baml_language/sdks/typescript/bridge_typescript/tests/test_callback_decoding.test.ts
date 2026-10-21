import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { decodeHostCall } from '../dist/proto.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';
import {
    _discardHostCallArgs,
    _handleRefcount,
    _seedFunctionRefHandle,
    _seedHeapHandle,
    _seedGenericMediaHandle,
} from '../dist/native.js';

const { BamlToHostCall, BamlHandleType } = baml_bridge.cffi.v1;

describe('host argument decoding ownership', () => {
    const captured: unknown[] = [];
    let previous: BamlTypeMap;
    let nextSlab = 0xDEC0DE;
    class Capture {
        constructor(fields: Record<string, unknown>) { captured.push(fields.value); }
    }
    class Reject {
        constructor() { throw new Error('deliberate argument validation failure'); }
    }
    const rejected = { classValue: { name: 'Reject' } };
    const wire = ([key, handleType]: ReturnType<typeof _seedFunctionRefHandle>) => ({ handleValue: { key, handleType } });
    const encode = (values: baml_bridge.cffi.v1.IBamlOutboundValue[]) => Buffer.from(
        BamlToHostCall.encode({ args: values.map((value) => ({ value })) }).finish(),
    );

    beforeEach(() => {
        previous = getTypeMap();
        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { Capture: () => Capture, Reject: () => Reject },
            enums: {}, typeAliases: {},
        }));
    });
    afterEach(() => { captured.length = 0; setTypeMap(previous); });

    test('decode failure releases untransferred callable and media', () => {
        const owners = [_seedFunctionRefHandle(126), _seedGenericMediaHandle()];
        try {
            expect(() => decodeHostCall(encode([rejected, ...owners.map(wire)]))).toThrow('deliberate argument');
            for (const [key] of owners) expect(_handleRefcount(key)).toBeNull();
        } finally {
            for (const owner of owners) {
                if (_handleRefcount(owner[0]) !== null) _discardHostCallArgs(encode([wire(owner)]));
            }
        }
    });

    test.each(['arguments', 'list', 'map', 'class', 'union'])(
        'decode failure preserves transferred owner and releases remaining: %s', (container) => {
            const slab = nextSlab++;
            const original = _seedHeapHandle(slab);
            const transferred = _seedHeapHandle(slab);
            const remaining = _seedHeapHandle(slab);
            expect(_handleRefcount(original[0])).toBe(3);
            const values = [
                { classValue: { name: 'Capture', fields: [{ key: 'value', value: wire(transferred) }] } },
                rejected, wire(remaining),
            ];
            let args: baml_bridge.cffi.v1.IBamlOutboundValue[];
            if (container === 'arguments') args = values;
            else if (container === 'map') args = [{ mapValue: { entries: values.map((value, index) => ({ key: String(index), value })) } }];
            else if (container === 'class') args = [{ classValue: { name: 'Capture', fields: values.map((value, index) => ({ key: String(index), value })) } }];
            else if (container === 'union') args = [{ unionVariantValue: { value: { listValue: { items: values } } } }];
            else args = [{ listValue: { items: values } }];
            try {
                expect(() => decodeHostCall(encode(args))).toThrow('deliberate argument');
                expect(captured).toHaveLength(1);
                expect(_handleRefcount(original[0])).toBe(2); // Original plus captured wrapper.
            } finally {
                // Release only untransferred/original wire owners. The captured
                // wrapper owns its reference until Node finalizes it.
                while ((_handleRefcount(original[0]) ?? 0) > 1) _discardHostCallArgs(encode([wire(original)]));
            }
        },
    );

    test('decode failure counts repeated keys and ignores borrowed host keys', () => {
        const slab = nextSlab++;
        const original = _seedHeapHandle(slab);
        const owners = [_seedHeapHandle(slab), _seedHeapHandle(slab)];
        const borrowed = [BamlHandleType.HOST_VALUE_CALLABLE, BamlHandleType.HOST_VALUE_OPAQUE]
            .map((handleType) => ({ handleValue: { key: original[0], handleType } }));
        try {
            expect(() => decodeHostCall(encode([rejected, ...owners.map(wire), ...borrowed])))
                .toThrow('deliberate argument');
            expect(_handleRefcount(original[0])).toBe(1); // Only the original owner remains.
        } finally {
            while (_handleRefcount(original[0]) !== null) _discardHostCallArgs(encode([wire(original)]));
        }
    });

    test('successful decode transfers ownership to callback arguments', () => {
        const owner = _seedGenericMediaHandle();
        const decoded = decodeHostCall(encode([wire(owner)]));
        expect(decoded.positional).toHaveLength(1);
        expect(_handleRefcount(owner[0])).toBe(1);
    });
});
