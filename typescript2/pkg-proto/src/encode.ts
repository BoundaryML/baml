import type {
  InboundValue,
  InboundMapEntry,
  CallFunctionArgs as CallFunctionArgsType,
} from './generated/baml_bridge/cffi/v1/baml_inbound';
import {
  CallFunctionArgs,
  FunctionOperation,
  InboundMapEntry as InboundMapEntryMessage,
} from './generated/baml_bridge/cffi/v1/baml_inbound';
import type { BamlSerializable } from './types';

function isBamlSerializable(val: unknown): val is BamlSerializable {
  return (
    typeof val === 'object' &&
    val !== null &&
    'toBaml' in val &&
    typeof (val as any).toBaml === 'function'
  );
}

function serializeValue(val: unknown): InboundValue {
  if (val === null || val === undefined) {
    return { valueType: undefined, value: undefined };
  }
  if (typeof val === 'string') {
    return { valueType: undefined, value: { $case: 'stringValue', stringValue: val } };
  }
  if (typeof val === 'number') {
    if (!Number.isFinite(val)) {
      throw new Error(`Cannot serialize non-finite number: ${val}`);
    }
    if (Number.isInteger(val)) {
      if (
        val > Number.MAX_SAFE_INTEGER ||
        val < Number.MIN_SAFE_INTEGER
      ) {
        console.warn(
          'Integer exceeds safe JS range; precision may be lost:',
          val,
        );
      }
      return { valueType: undefined, value: { $case: 'intValue', intValue: val } };
    }
    return { valueType: undefined, value: { $case: 'floatValue', floatValue: val } };
  }
  if (typeof val === 'bigint') {
    // Base-sixteen hex on the wire (no `0x` prefix, leading `-` for
    // negatives) — matches Rust's `format!("{:x}")` / `parse_bytes(_, 16)`.
    return { valueType: undefined, value: { $case: 'bigintValue', bigintValue: val.toString(16) } };
  }
  if (typeof val === 'boolean') {
    return { valueType: undefined, value: { $case: 'boolValue', boolValue: val } };
  }
  if (Array.isArray(val)) {
    return {
      valueType: undefined,
      value: {
        $case: 'listValue',
        listValue: { values: val.map(serializeValue) },
      },
    };
  }
  if (typeof val === 'object') {
    if (isBamlSerializable(val)) {
      return val.toBaml();
    }
    const bamlMarker = (val as Record<string, unknown>)['$baml'];
    // Honour a `$baml: { enum: 'user.Color', value: 'Red' }` marker so hosts
    // (e.g. the playground args form) can pass real enum variants. Nothing on
    // the args path coerces a plain string into an enum variant, so without
    // this an expr function's `param == Color.Red` is silently false. The
    // enum name is passed verbatim: the engine resolves its registered FQN
    // (`user.ns.Color`) directly and falls back to prepending `user.`.
    if (bamlMarker && typeof bamlMarker === 'object' && 'enum' in bamlMarker) {
      const marker = bamlMarker as { enum?: unknown; value?: unknown };
      // Fail fast on a malformed marker: falling through would serialize the
      // `$baml` object as a literal map entry, which the engine accepts
      // silently and misinterprets.
      if (
        typeof marker.enum !== 'string' ||
        typeof marker.value !== 'string'
      ) {
        throw new Error(
          'Invalid $baml enum marker: expected { enum: string, value: string }',
        );
      }
      return {
        valueType: undefined,
        value: {
          $case: 'enumValue',
          enumValue: { name: marker.enum, value: marker.value },
        },
      };
    }
    // Honour a `$baml: { type: 'ClassName' }` (or `'Namespace::ClassName'`)
    // marker so JSON shaped like `decodeCallResult` output round-trips back as
    // a `classValue`. Without this, every plain object would serialize as a
    // map, making typed function calls (e.g. `(inv: Invoice)`) fail with
    // "expected instance, got map" inside the runtime.
    if (
      bamlMarker &&
      typeof bamlMarker === 'object' &&
      typeof (bamlMarker as Record<string, unknown>).type === 'string'
    ) {
      const typeStr = (bamlMarker as { type: string }).type;
      const sepIdx = typeStr.lastIndexOf('::');
      const className =
        sepIdx >= 0 ? typeStr.slice(sepIdx + 2) : typeStr;
      const fields: InboundMapEntry[] = Object.entries(val)
        .filter(([k]) => k !== '$baml')
        .map(([k, v]) => ({
          key: { $case: 'stringKey' as const, stringKey: k },
          value: serializeValue(v),
        }));
      return {
        valueType: {
          ty: {
            $case: 'classTy',
            classTy: { name: className, typeArgs: [] },
          },
        },
        value: {
          $case: 'classValue',
          classValue: { fields },
        },
      };
    }
    // Plain object → map with string keys
    const entries: InboundMapEntry[] = Object.entries(val).map(
      ([k, v]) => ({
        key: { $case: 'stringKey' as const, stringKey: k },
        value: serializeValue(v),
      }),
    );
    return {
      valueType: undefined,
      value: {
        $case: 'mapValue',
        mapValue: { entries },
      },
    };
  }
  throw new Error(
    `Cannot serialize value of type ${typeof val} to BAML`,
  );
}

export function encodeCallArgs(
  kwargs: Record<string, unknown>,
  callId: number,
): Uint8Array {
  const entries: InboundMapEntry[] = Object.entries(kwargs).map(
    ([k, v]) => ({
      key: { $case: 'stringKey' as const, stringKey: k },
      value: serializeValue(v),
    }),
  );

  const args: CallFunctionArgsType = {
    kwargs: entries,
    callId,
    // Generic TypeVar bindings (`type_args`) — unused by this playground
    // encoder, which only sends positional kwargs.
    typeArgs: [],
    operation: FunctionOperation.DIRECT,
  };

  return CallFunctionArgs.encode(args).finish();
}

export function encodeRunArgs(kwargs: Record<string, unknown>): Uint8Array {
  const chunks = Object.entries(kwargs).map(([k, v]) => {
    const entry: InboundMapEntry = {
      key: { $case: 'stringKey' as const, stringKey: k },
      value: serializeValue(v),
    };
    const bytes = InboundMapEntryMessage.encode(entry).finish();
    return concatBytes([encodeVarint(bytes.length), bytes]);
  });
  return concatBytes(chunks);
}

function encodeVarint(value: number): Uint8Array {
  const bytes: number[] = [];
  let remaining = value;
  while (remaining >= 0x80) {
    bytes.push((remaining & 0x7f) | 0x80);
    remaining = Math.floor(remaining / 0x80);
  }
  bytes.push(remaining);
  return new Uint8Array(bytes);
}

function concatBytes(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

export { serializeValue };
