import { describe, it, expect } from 'vitest';
import { encodeCallArgs, encodeRunArgs, decodeCallResult, serializeValue, deserializeValue } from '../index';
import {
  CallFunctionArgs,
  FunctionOperation,
  InboundMapEntry,
} from '../generated/baml_bridge/cffi/v1/baml_inbound';
import { BamlHandleType } from '../generated/baml_bridge/cffi/v1/baml_handle';
import { BamlOutboundValue, MediaTypeEnum } from '../generated/baml_bridge/cffi/v1/baml_outbound';
import { BamlTyPrimitiveKind } from '../generated/baml_bridge/cffi/v1/baml_type';

function decodeDelimitedEntries(bytes: Uint8Array): InboundMapEntry[] {
  const entries: InboundMapEntry[] = [];
  let offset = 0;
  while (offset < bytes.length) {
    const { value: length, nextOffset } = readVarint(bytes, offset);
    offset = nextOffset;
    const end = offset + length;
    entries.push(InboundMapEntry.decode(bytes.slice(offset, end)));
    offset = end;
  }
  return entries;
}

function readVarint(bytes: Uint8Array, offset: number): { value: number; nextOffset: number } {
  let value = 0;
  let shift = 0;
  let cursor = offset;
  while (cursor < bytes.length) {
    const byte = bytes[cursor++];
    value += (byte & 0x7f) * 2 ** shift;
    if ((byte & 0x80) === 0) {
      return { value, nextOffset: cursor };
    }
    shift += 7;
  }
  throw new Error('unterminated varint');
}

describe('encodeCallArgs', () => {
  it('encodes an unsorted array as function kwargs', () => {
    const bytes = encodeCallArgs({ arr: [5, 3, 1, 4, 2] }, 123);
    expect(bytes).toBeInstanceOf(Uint8Array);
    expect(bytes.length).toBeGreaterThan(0);

    // Decode back to proto to verify structure
    const decoded = CallFunctionArgs.decode(bytes);
    expect(decoded.callId).toBe(123);
    expect(decoded.operation).toBe(FunctionOperation.DIRECT);
    expect(decoded.kwargs).toHaveLength(1);

    const kwarg = decoded.kwargs[0];
    expect(kwarg.key?.$case).toBe('stringKey');
    if (kwarg.key?.$case === 'stringKey') {
      expect(kwarg.key.stringKey).toBe('arr');
    }

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

  it('encodes run args without the CallFunctionArgs call id wrapper', () => {
    const bytes = encodeRunArgs({ name: 'Ada', count: 2 });
    const entries = decodeDelimitedEntries(bytes);

    expect(() => CallFunctionArgs.decode(bytes)).toThrow();
    expect(entries.map((entry) => entry.key)).toEqual([
      { $case: 'stringKey', stringKey: 'name' },
      { $case: 'stringKey', stringKey: 'count' },
    ]);
    expect(entries[0].value?.value).toEqual({
      $case: 'stringValue',
      stringValue: 'Ada',
    });
    expect(entries[1].value?.value).toEqual({
      $case: 'intValue',
      intValue: 2,
    });
  });

  it('encodes various JS types correctly', () => {
    const bytes = encodeCallArgs({
      name: 'Alice',
      age: 30,
      score: 99.5,
      active: true,
      nothing: null,
    }, 124);
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
    }, 125);
    const decoded = CallFunctionArgs.decode(bytes);
    const userVal = decoded.kwargs[0].value;
    expect(userVal?.value?.$case).toBe('mapValue');
  });

  it('uses toBaml() when available', () => {
    const custom = {
      toBaml() {
        return {
          valueType: {
            ty: {
              $case: 'classTy' as const,
              classTy: { name: 'MyClass', typeArgs: [] },
            },
          },
          value: {
            $case: 'classValue',
            classValue: {
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
    const bytes = encodeCallArgs({ obj: custom }, 126);
    const decoded = CallFunctionArgs.decode(bytes);
    const val = decoded.kwargs[0].value;
    expect(val?.value?.$case).toBe('classValue');
    expect(val?.valueType?.ty).toEqual({
      $case: 'classTy',
      classTy: { name: 'MyClass', typeArgs: [] },
    });
  });

  it('preserves a sparse exact literal type on the value node', () => {
    const draft = {
      toBaml() {
        return {
          valueType: {
            ty: {
              $case: 'literal' as const,
              literal: {
                literal: {
                  $case: 'stringValue' as const,
                  stringValue: 'draft',
                },
              },
            },
          },
          value: {
            $case: 'stringValue' as const,
            stringValue: 'draft',
          },
        };
      },
    };

    const decoded = CallFunctionArgs.decode(encodeCallArgs({ status: draft }, 131));
    const value = decoded.kwargs[0].value;
    expect(value?.valueType?.ty).toEqual({
      $case: 'literal',
      literal: {
        literal: { $case: 'stringValue', stringValue: 'draft' },
      },
    });
    expect(value?.value).toEqual({
      $case: 'stringValue',
      stringValue: 'draft',
    });
  });

  it('encodes a $baml enum marker as an enumValue', () => {
    const bytes = encodeCallArgs(
      { c: { $baml: { enum: 'user.Color', value: 'Red' } } },
      127,
    );
    const decoded = CallFunctionArgs.decode(bytes);
    const val = decoded.kwargs[0].value;
    expect(val?.value?.$case).toBe('enumValue');
    if (val?.value?.$case === 'enumValue') {
      expect(val.value.enumValue.name).toBe('user.Color');
      expect(val.value.enumValue.value).toBe('Red');
    }
  });

  it('rejects malformed $baml enum markers instead of emitting a map', () => {
    expect(() =>
      encodeCallArgs({ c: { $baml: { enum: 'user.Color' } } }, 129),
    ).toThrow(/enum marker/);
    expect(() =>
      encodeCallArgs({ c: { $baml: { enum: 'user.Color', value: 3 } } }, 130),
    ).toThrow(/enum marker/);
  });

  it('encodes enum markers in nested positions (list element, class field)', () => {
    const bytes = encodeCallArgs(
      {
        box: {
          $baml: { type: 'user.Box' },
          colors: [{ $baml: { enum: 'user.Color', value: 'Green' } }],
        },
      },
      128,
    );
    const decoded = CallFunctionArgs.decode(bytes);
    const val = decoded.kwargs[0].value;
    expect(val?.value?.$case).toBe('classValue');
    if (val?.value?.$case !== 'classValue') return;
    const colors = val.value.classValue.fields[0]?.value;
    expect(colors?.value?.$case).toBe('listValue');
    if (colors?.value?.$case !== 'listValue') return;
    const first = colors.value.listValue.values[0];
    expect(first?.value?.$case).toBe('enumValue');
    if (first?.value?.$case === 'enumValue') {
      expect(first.value.enumValue.name).toBe('user.Color');
      expect(first.value.enumValue.value).toBe('Green');
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
          itemType: { ty: { $case: 'primitive', primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT } } },
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
          name: 'Person',
          typeArgs: [],
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
          name: 'Color',
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
          keyType: { ty: { $case: 'primitive', primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_STRING } } },
          valueType: { ty: { $case: 'primitive', primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT } } },
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
          literal: { $case: 'stringValue', stringValue: 'fixed' },
        },
      },
    });
    expect(decodeCallResult(strBytes, defaultWrapHandle)).toBe('fixed');

    const boolBytes = encodeResult({
      value: {
        $case: 'literalValue',
        literalValue: {
          literal: { $case: 'boolValue', boolValue: true },
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
          name: 'StringOrInt',
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
        handleValue: {
          key: 42,
          handleType: BamlHandleType.FUNCTION_REF,
          ty: undefined,
        },
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

  // Coverage checklist — all BamlOutboundValue.$case values:
  // [x] nullValue, [x] stringValue, [x] intValue (via listValue), [x] floatValue (via encodeCallArgs),
  // [x] boolValue (via encodeCallArgs), [x] classValue, [x] enumValue, [x] literalValue,
  // [x] listValue, [x] mapValue, [x] unionVariantValue, [x] handleValue,
  // [x] uint8arrayValue, [x] mediaValue, [x] promptAstValue

  it('decodes a uint8array', () => {
    const sampleBytes = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
    const bytes = encodeResult({
      value: { $case: 'uint8arrayValue', uint8arrayValue: sampleBytes },
    });
    const result = decodeCallResult(bytes, defaultWrapHandle);
    expect(result).toBeInstanceOf(Uint8Array);
    expect(result).toEqual(sampleBytes);
  });

  it('decodes media (url type)', () => {
    const bytes = encodeResult({
      value: {
        $case: 'mediaValue',
        mediaValue: {
          media: MediaTypeEnum.IMAGE,
          mimeType: 'image/png',
          value: { $case: 'url', url: 'https://example.com/img.png' },
        },
      },
    });
    const result = decodeCallResult(bytes, defaultWrapHandle) as Record<string, unknown>;
    expect(result).toEqual({
      $baml: { type: '$media' },
      media_type: 'image',
      mime_type: 'image/png',
      content_type: 'url',
      url: 'https://example.com/img.png',
    });
  });

  it('decodes prompt ast (simple string)', () => {
    const bytes = encodeResult({
      value: {
        $case: 'promptAstValue',
        promptAstValue: {
          value: {
            $case: 'simple',
            simple: {
              value: { $case: 'string', string: 'Hello prompt' },
            },
          },
        },
      },
    });
    const result = decodeCallResult(bytes, defaultWrapHandle) as Record<string, unknown>;
    expect(result).toEqual({
      $baml: { type: '$prompt_ast' },
      content_type: 'simple',
      value: {
        $baml: { type: '$prompt_ast_simple' },
        content_type: 'string',
        value: 'Hello prompt',
      },
    });
  });
});

describe('round-trip: encode bubble sort args', () => {
  it('encodes the unsorted array that would be passed to BubbleSort', () => {
    const unsorted = [5, 3, 1, 4, 2];
    const bytes = encodeCallArgs({ arr: unsorted }, 127);

    // Verify it's valid protobuf
    const decoded = CallFunctionArgs.decode(bytes);
    expect(decoded.kwargs).toHaveLength(1);

    // Simulate what the WASM runtime would return: a sorted array
    const sortedResult = BamlOutboundValue.encode({
      value: {
        $case: 'listValue',
        listValue: {
          itemType: { ty: { $case: 'primitive', primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT } } },
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

describe('structuredClone round-trip', () => {
  function encodeResult(holder: Parameters<typeof BamlOutboundValue.encode>[0]): Uint8Array {
    return BamlOutboundValue.encode(holder).finish();
  }

  const cloneWrapHandle = (key: bigint, handleType: number, typeName: string) => ({
    handle_key: key,
    handle_type: handleType,
    type_name: typeName,
  });

  it('primitives survive structured clone', () => {
    const cases = [
      { value: { $case: 'nullValue' as const, nullValue: {} } },
      { value: { $case: 'stringValue' as const, stringValue: 'hello' } },
      { value: { $case: 'intValue' as const, intValue: 42 } },
      { value: { $case: 'boolValue' as const, boolValue: true } },
    ];
    for (const c of cases) {
      const bytes = encodeResult(c);
      const decoded = decodeCallResult(bytes, cloneWrapHandle);
      expect(structuredClone(decoded)).toEqual(decoded);
    }
  });

  it('Uint8Array survives structured clone', () => {
    const sampleBytes = new Uint8Array([1, 2, 3, 4, 5]);
    const bytes = encodeResult({
      value: { $case: 'uint8arrayValue', uint8arrayValue: sampleBytes },
    });
    const decoded = decodeCallResult(bytes, cloneWrapHandle);
    const cloned = structuredClone(decoded);
    expect(cloned).toBeInstanceOf(Uint8Array);
    expect(cloned).toEqual(decoded);
  });

  it('class with nested fields survives structured clone', () => {
    const bytes = encodeResult({
      value: {
        $case: 'classValue',
        classValue: {
          name: 'Person',
          typeArgs: [],
          fields: [
            { key: 'name', value: { value: { $case: 'stringValue', stringValue: 'Alice' } } },
            { key: 'age', value: { value: { $case: 'intValue', intValue: 30 } } },
          ],
        },
      },
    });
    const decoded = decodeCallResult(bytes, cloneWrapHandle);
    expect(structuredClone(decoded)).toEqual(decoded);
  });

  it('handle with bigint key survives structured clone', () => {
    const bytes = encodeResult({
      value: {
        $case: 'handleValue',
        handleValue: {
          key: 42,
          handleType: BamlHandleType.FUNCTION_REF,
          ty: undefined,
        },
      },
    });
    const decoded = decodeCallResult(bytes, cloneWrapHandle);
    const cloned = structuredClone(decoded);
    expect(cloned).toEqual(decoded);
    // Verify bigint is preserved
    const handle = (cloned as { handle: { handle_key: bigint } }).handle;
    expect(typeof handle.handle_key).toBe('bigint');
  });

  it('complex nested value survives structured clone', () => {
    const bytes = encodeResult({
      value: {
        $case: 'classValue',
        classValue: {
          name: 'ComplexResult',
          typeArgs: [],
          fields: [
            {
              key: 'items',
              value: {
                value: {
                  $case: 'listValue',
                  listValue: {
                    itemType: { ty: { $case: 'primitive', primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT } } },
                    items: [
                      { value: { $case: 'intValue', intValue: 1 } },
                      { value: { $case: 'intValue', intValue: 2 } },
                    ],
                  },
                },
              },
            },
            {
              key: 'data',
              value: {
                value: { $case: 'uint8arrayValue', uint8arrayValue: new Uint8Array([10, 20, 30]) },
              },
            },
            {
              key: 'ref',
              value: {
                value: {
                  $case: 'handleValue',
                  handleValue: {
                    key: 99,
                    handleType: BamlHandleType.FUNCTION_REF,
                    ty: undefined,
                  },
                },
              },
            },
            {
              key: 'image',
              value: {
                value: {
                  $case: 'mediaValue',
                  mediaValue: {
                    media: MediaTypeEnum.IMAGE,
                    mimeType: 'image/png',
                    value: { $case: 'url', url: 'https://example.com/img.png' },
                  },
                },
              },
            },
          ],
        },
      },
    });
    const decoded = decodeCallResult(bytes, cloneWrapHandle);
    const cloned = structuredClone(decoded);
    expect(cloned).toEqual(decoded);
  });
});
