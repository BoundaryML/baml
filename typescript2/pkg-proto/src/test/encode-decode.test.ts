import { describe, it, expect } from 'vitest';
import { encodeCallArgs, decodeCallResult, serializeValue, deserializeValue } from '../index';
import { CallFunctionArgs, BamlHandleType } from '../generated/baml/cffi/v1/baml_inbound';
import { BamlOutboundValue } from '../generated/baml/cffi/v1/baml_outbound';

describe('encodeCallArgs', () => {
  it('encodes an unsorted array as function kwargs', () => {
    const bytes = encodeCallArgs({ arr: [5, 3, 1, 4, 2] });
    expect(bytes).toBeInstanceOf(Uint8Array);
    expect(bytes.length).toBeGreaterThan(0);

    // Decode back to proto to verify structure
    const decoded = CallFunctionArgs.decode(bytes);
    expect(decoded.kwargs).toHaveLength(1);

    const kwarg = decoded.kwargs[0];
    expect(kwarg.key?.$case).toBe('stringKey');
    expect(kwarg.key?.stringKey).toBe('arr');

    const val = kwarg.value;
    expect(val?.value?.$case).toBe('listValue');
    if (val?.value?.$case === 'listValue') {
      const items = val.value.listValue.values;
      expect(items).toHaveLength(5);
      expect(items[0].value?.$case).toBe('intValue');
      if (items[0].value?.$case === 'intValue') {
        expect(items[0].value.intValue).toBe(5);
      }
    }
  });

  it('encodes various JS types correctly', () => {
    const bytes = encodeCallArgs({
      name: 'Alice',
      age: 30,
      score: 99.5,
      active: true,
      nothing: null,
    });
    const decoded = CallFunctionArgs.decode(bytes);
    expect(decoded.kwargs).toHaveLength(5);

    const byKey = new Map(
      decoded.kwargs.map((k) => [
        k.key?.$case === 'stringKey' ? k.key.stringKey : '',
        k.value,
      ]),
    );

    expect(byKey.get('name')?.value?.$case).toBe('stringValue');
    expect(byKey.get('age')?.value?.$case).toBe('intValue');
    expect(byKey.get('score')?.value?.$case).toBe('floatValue');
    expect(byKey.get('active')?.value?.$case).toBe('boolValue');
    expect(byKey.get('nothing')?.value).toBeUndefined(); // null = no value
  });

  it('encodes nested objects as maps', () => {
    const bytes = encodeCallArgs({
      user: { name: 'Bob', scores: [10, 20] },
    });
    const decoded = CallFunctionArgs.decode(bytes);
    const userVal = decoded.kwargs[0].value;
    expect(userVal?.value?.$case).toBe('mapValue');
  });

  it('uses toBaml() when available', () => {
    const custom = {
      toBaml() {
        return {
          value: {
            $case: 'classValue',
            classValue: {
              name: 'MyClass',
              fields: [
                {
                  key: { $case: 'stringKey', stringKey: 'x' },
                  value: { value: { $case: 'intValue', intValue: 42 } },
                },
              ],
            },
          },
        };
      },
    };
    const bytes = encodeCallArgs({ obj: custom });
    const decoded = CallFunctionArgs.decode(bytes);
    const val = decoded.kwargs[0].value;
    expect(val?.value?.$case).toBe('classValue');
    if (val?.value?.$case === 'classValue') {
      expect(val.value.classValue.name).toBe('MyClass');
    }
  });
});

describe('decodeCallResult', () => {
  function encodeResult(holder: Parameters<typeof BamlOutboundValue.encode>[0]): Uint8Array {
    return BamlOutboundValue.encode(holder).finish();
  }

  const defaultWrapHandle = (_key: bigint, _handleType: number, typeName: string) => ({ handle_type: typeName });

  it('decodes a sorted int array', () => {
    const bytes = encodeResult({
      value: {
        $case: 'listValue',
        listValue: {
          itemType: { type: { $case: 'intType', intType: {} } },
          items: [
            { value: { $case: 'intValue', intValue: 1 } },
            { value: { $case: 'intValue', intValue: 2 } },
            { value: { $case: 'intValue', intValue: 3 } },
            { value: { $case: 'intValue', intValue: 4 } },
            { value: { $case: 'intValue', intValue: 5 } },
          ],
        },
      },
    });

    const result = decodeCallResult(bytes, defaultWrapHandle);
    expect(result).toEqual([1, 2, 3, 4, 5]);
  });

  it('decodes a string', () => {
    const bytes = encodeResult({
      value: { $case: 'stringValue', stringValue: 'hello world' },
    });
    expect(decodeCallResult(bytes, defaultWrapHandle)).toBe('hello world');
  });

  it('decodes null', () => {
    const bytes = encodeResult({
      value: { $case: 'nullValue', nullValue: {} },
    });
    expect(decodeCallResult(bytes, defaultWrapHandle)).toBeNull();
  });

  it('decodes a class with $baml.type', () => {
    const bytes = encodeResult({
      value: {
        $case: 'classValue',
        classValue: {
          name: { namespace: 1, name: 'Person' },
          fields: [
            {
              key: 'name',
              value: { value: { $case: 'stringValue', stringValue: 'Alice' } },
            },
            {
              key: 'age',
              value: { value: { $case: 'intValue', intValue: 30 } },
            },
          ],
        },
      },
    });
    const result = decodeCallResult(bytes, defaultWrapHandle);
    expect(result).toEqual({
      $baml: { type: 'Person' },
      name: 'Alice',
      age: 30,
    });
  });

  it('decodes an enum as a plain string', () => {
    const bytes = encodeResult({
      value: {
        $case: 'enumValue',
        enumValue: {
          name: { namespace: 1, name: 'Color' },
          value: 'RED',
          isDynamic: false,
        },
      },
    });
    expect(decodeCallResult(bytes, defaultWrapHandle)).toBe('RED');
  });

  it('decodes a map', () => {
    const bytes = encodeResult({
      value: {
        $case: 'mapValue',
        mapValue: {
          keyType: { type: { $case: 'stringType', stringType: {} } },
          valueType: { type: { $case: 'intType', intType: {} } },
          entries: [
            {
              key: 'a',
              value: { value: { $case: 'intValue', intValue: 1 } },
            },
            {
              key: 'b',
              value: { value: { $case: 'intValue', intValue: 2 } },
            },
          ],
        },
      },
    });
    expect(decodeCallResult(bytes, defaultWrapHandle)).toEqual({ a: 1, b: 2 });
  });

  it('decodes literals to their primitive values', () => {
    const strBytes = encodeResult({
      value: {
        $case: 'literalValue',
        literalValue: {
          literal: { $case: 'stringLiteral', stringLiteral: { value: 'fixed' } },
        },
      },
    });
    expect(decodeCallResult(strBytes, defaultWrapHandle)).toBe('fixed');

    const boolBytes = encodeResult({
      value: {
        $case: 'literalValue',
        literalValue: {
          literal: { $case: 'boolLiteral', boolLiteral: { value: true } },
        },
      },
    });
    expect(decodeCallResult(boolBytes, defaultWrapHandle)).toBe(true);
  });

  it('unwraps union variants', () => {
    const bytes = encodeResult({
      value: {
        $case: 'unionVariantValue',
        unionVariantValue: {
          name: { namespace: 1, name: 'StringOrInt' },
          isOptional: false,
          isSinglePattern: false,
          selfType: undefined,
          valueOptionName: 'stringValue',
          value: { value: { $case: 'stringValue', stringValue: 'hi' } },
        },
      },
    });
    expect(decodeCallResult(bytes, defaultWrapHandle)).toBe('hi');
  });

  it('calls wrapHandle when handle value encountered', () => {
    const bytes = encodeResult({
      value: {
        $case: 'handleValue',
        handleValue: { key: 42, handleType: BamlHandleType.FUNCTION_REF },
      },
    });
    const result = decodeCallResult(bytes, (key, handleType, typeName) => {
      expect(key).toBe(42n);
      expect(handleType).toBe(BamlHandleType.FUNCTION_REF);
      expect(typeName).toBe('function_ref');
      return { kind: 'functionRef', key: 42n };
    });
    expect(result).toEqual({
      $baml: { type: '$handle' },
      handle: { kind: 'functionRef', key: 42n },
    });
  });
});

describe('round-trip: encode bubble sort args', () => {
  it('encodes the unsorted array that would be passed to BubbleSort', () => {
    const unsorted = [5, 3, 1, 4, 2];
    const bytes = encodeCallArgs({ arr: unsorted });

    // Verify it's valid protobuf
    const decoded = CallFunctionArgs.decode(bytes);
    expect(decoded.kwargs).toHaveLength(1);

    // Simulate what the WASM runtime would return: a sorted array
    const sortedResult = BamlOutboundValue.encode({
      value: {
        $case: 'listValue',
        listValue: {
          itemType: { type: { $case: 'intType', intType: {} } },
          items: [...unsorted]
            .sort((a, b) => a - b)
            .map((n) => ({
              value: { $case: 'intValue' as const, intValue: n },
            })),
        },
      },
    }).finish();

    const result = decodeCallResult(sortedResult, (_key, _ht, typeName) => ({ handle_type: typeName }));
    expect(result).toEqual([1, 2, 3, 4, 5]);
  });
});
