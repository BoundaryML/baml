import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { makeHostCallableDispatch } from '../dist/proto.js';
import {
    _discardHostCallArgs,
    _handleRefcount,
    _seedFunctionRefHandle,
    _seedHeapHandle,
} from '../dist/native.js';

const { BamlToHostCall, BamlHandleType } = baml_bridge.cffi.v1;

describe('cancelled host dispatch wire ownership', () => {
    test('an expired dispatch releases its args without running user code', () => {
        const [key, handleType] = _seedFunctionRefHandle(120);
        const callback = vi.fn();
        const args = Buffer.from(BamlToHostCall.encode({
            args: [{ value: { handleValue: { key, handleType } } }],
        }).finish());
        // Zero is never allocated as an in-flight host-call ID.
        makeHostCallableDispatch(callback)(0, args);
        expect(callback).not.toHaveBeenCalled();
        expect(_handleRefcount(key)).toBeNull();
    });

    test('discard walks lists, maps, classes and unions', () => {
        const owners = [121, 122, 123, 124].map((index) => _seedFunctionRefHandle(index));
        const values = owners.map(([key, handleType]) => ({ handleValue: { key, handleType } }));
        const args = Buffer.from(BamlToHostCall.encode({ args: [
            { value: { listValue: { items: [values[0]] } } },
            { value: { mapValue: { entries: [{ key: 'map', value: values[1] }] } } },
            { value: { classValue: { name: 'Unmapped', fields: [{ key: 'field', value: values[2] }] } } },
            { value: { unionVariantValue: { value: { listValue: { items: [values[3]] } } } } },
        ] }).finish());
        _discardHostCallArgs(args);
        for (const [key] of owners) expect(_handleRefcount(key)).toBeNull();
    });

    test('borrowed host keys do not release a coincident native key', () => {
        const [key, handleType] = _seedFunctionRefHandle(125);
        const args = Buffer.from(BamlToHostCall.encode({ args: [
            { value: { handleValue: { key, handleType: BamlHandleType.HOST_VALUE_CALLABLE } } },
            { value: { handleValue: { key, handleType: BamlHandleType.HOST_VALUE_OPAQUE } } },
        ] }).finish());
        _discardHostCallArgs(args);
        expect(_handleRefcount(key)).toBe(1);
        _discardHostCallArgs(Buffer.from(BamlToHostCall.encode({
            args: [{ value: { handleValue: { key, handleType } } }],
        }).finish()));
        expect(_handleRefcount(key)).toBeNull();
    });

    test('discard releases exactly one owner of a shared heap key', () => {
        const [key, handleType] = _seedHeapHandle(8001);
        const [shared] = _seedHeapHandle(8001);
        expect(shared).toEqual(key);
        expect(_handleRefcount(key)).toBe(2);
        const args = Buffer.from(BamlToHostCall.encode({
            args: [{ value: { handleValue: { key, handleType } } }],
        }).finish());
        _discardHostCallArgs(args);
        expect(_handleRefcount(key)).toBe(1);
        _discardHostCallArgs(args);
        expect(_handleRefcount(key)).toBeNull();
    });
});
