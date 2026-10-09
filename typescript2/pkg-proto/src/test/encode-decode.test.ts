import { describe, expect, it } from 'vitest';
import { BamlHandleType } from '../generated/baml_bridge/cffi/v1/baml_handle';
import {
  BamlOutboundValue,
  MediaTypeEnum,
} from '../generated/baml_bridge/cffi/v1/baml_outbound';
import { BamlTyPrimitiveKind } from '../generated/baml_bridge/cffi/v1/baml_type';
import { decodeCallResult } from '../index';

describe('decodeCallResult', () => {
  function encodeResult(
    holder: Parameters<typeof BamlOutboundValue.encode>[0],
  ): Uint8Array {
    return BamlOutboundValue.encode(holder).finish();
  }

  const defaultWrapHandle = (
    _key: bigint,
    _handleType: number,
    typeName: string,
  ) => ({ handle_type: typeName });

  it('decodes a sorted int array', () => {
    const bytes = encodeResult({
      value: {
        $case: 'listValue',
        listValue: {
          items: [
            { value: { $case: 'intValue', intValue: 1 } },
            { value: { $case: 'intValue', intValue: 2 } },
            { value: { $case: 'intValue', intValue: 3 } },
            { value: { $case: 'intValue', intValue: 4 } },
            { value: { $case: 'intValue', intValue: 5 } },
          ],
          itemType: {
            ty: {
              $case: 'primitive',
              primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT },
            },
          },
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
          name: 'Person',
          typeArgs: [],
        },
      },
    });
    const result = decodeCallResult(bytes, defaultWrapHandle);
    expect(result).toEqual({
      $baml: { type: 'Person' },
      age: 30,
      name: 'Alice',
    });
  });

  it('decodes an enum as a plain string', () => {
    const bytes = encodeResult({
      value: {
        $case: 'enumValue',
        enumValue: {
          isDynamic: false,
          name: 'Color',
          value: 'RED',
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
          keyType: {
            ty: {
              $case: 'primitive',
              primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_STRING },
            },
          },
          valueType: {
            ty: {
              $case: 'primitive',
              primitive: { kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT },
            },
          },
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
          isOptional: false,
          isSinglePattern: false,
          name: 'StringOrInt',
          selfType: undefined,
          value: { value: { $case: 'stringValue', stringValue: 'hi' } },
          valueOptionName: 'stringValue',
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
          handleType: BamlHandleType.FUNCTION_REF,
          key: 42,
          ty: undefined,
        },
      },
    });
    const result = decodeCallResult(bytes, (key, handleType, typeName) => {
      expect(key).toBe(42n);
      expect(handleType).toBe(BamlHandleType.FUNCTION_REF);
      expect(typeName).toBe('function_ref');
      return { key: 42n, kind: 'functionRef' };
    });
    expect(result).toEqual({
      $baml: { type: '$handle' },
      handle: { key: 42n, kind: 'functionRef' },
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
    const result = decodeCallResult(bytes, defaultWrapHandle) as Record<
      string,
      unknown
    >;
    expect(result).toEqual({
      $baml: { type: '$media' },
      content_type: 'url',
      media_type: 'image',
      mime_type: 'image/png',
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
    const result = decodeCallResult(bytes, defaultWrapHandle) as Record<
      string,
      unknown
    >;
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

describe('structuredClone round-trip', () => {
  function encodeResult(
    holder: Parameters<typeof BamlOutboundValue.encode>[0],
  ): Uint8Array {
    return BamlOutboundValue.encode(holder).finish();
  }

  const cloneWrapHandle = (
    key: bigint,
    handleType: number,
    typeName: string,
  ) => ({
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
          name: 'Person',
          typeArgs: [],
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
          handleType: BamlHandleType.FUNCTION_REF,
          key: 42,
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
          fields: [
            {
              key: 'items',
              value: {
                value: {
                  $case: 'listValue',
                  listValue: {
                    items: [
                      { value: { $case: 'intValue', intValue: 1 } },
                      { value: { $case: 'intValue', intValue: 2 } },
                    ],
                    itemType: {
                      ty: {
                        $case: 'primitive',
                        primitive: {
                          kind: BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT,
                        },
                      },
                    },
                  },
                },
              },
            },
            {
              key: 'data',
              value: {
                value: {
                  $case: 'uint8arrayValue',
                  uint8arrayValue: new Uint8Array([10, 20, 30]),
                },
              },
            },
            {
              key: 'ref',
              value: {
                value: {
                  $case: 'handleValue',
                  handleValue: {
                    handleType: BamlHandleType.FUNCTION_REF,
                    key: 99,
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
          name: 'ComplexResult',
          typeArgs: [],
        },
      },
    });
    const decoded = decodeCallResult(bytes, cloneWrapHandle);
    const cloned = structuredClone(decoded);
    expect(cloned).toEqual(decoded);
  });
});
