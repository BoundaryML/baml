import type {
  InboundValue,
  InboundMapEntry,
  CallFunctionArgs as CallFunctionArgsType,
} from './generated/baml_core/cffi/v1/baml_inbound';
import {
  CallFunctionArgs,
} from './generated/baml_core/cffi/v1/baml_inbound';
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
    return { value: undefined };
  }
  if (typeof val === 'string') {
    return { value: { $case: 'stringValue', stringValue: val } };
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
      return { value: { $case: 'intValue', intValue: val } };
    }
    return { value: { $case: 'floatValue', floatValue: val } };
  }
  if (typeof val === 'bigint') {
    // Base-sixteen hex on the wire (no `0x` prefix, leading `-` for
    // negatives) — matches Rust's `format!("{:x}")` / `parse_bytes(_, 16)`.
    return { value: { $case: 'bigintValue', bigintValue: val.toString(16) } };
  }
  if (typeof val === 'boolean') {
    return { value: { $case: 'boolValue', boolValue: val } };
  }
  if (Array.isArray(val)) {
    return {
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
    // Honour a `$baml: { type: 'ClassName' }` (or `'Namespace::ClassName'`)
    // marker so JSON shaped like `decodeCallResult` output round-trips back as
    // a `classValue`. Without this, every plain object would serialize as a
    // map, making typed function calls (e.g. `(inv: Invoice)`) fail with
    // "expected instance, got map" inside the runtime.
    const bamlMarker = (val as Record<string, unknown>)['$baml'];
    if (
      bamlMarker &&
      typeof bamlMarker === 'object' &&
      typeof (bamlMarker as Record<string, unknown>).type === 'string'
    ) {
      const typeStr = (bamlMarker as { type: string }).type;
      const sepIdx = typeStr.indexOf('::');
      const className =
        sepIdx >= 0 ? typeStr.slice(sepIdx + 2) : typeStr;
      const fields: InboundMapEntry[] = Object.entries(val)
        .filter(([k]) => k !== '$baml')
        .map(([k, v]) => ({
          key: { $case: 'stringKey' as const, stringKey: k },
          value: serializeValue(v),
        }));
      return {
        value: {
          $case: 'classValue',
          classValue: { name: className, fields },
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
      value: { $case: 'mapValue', mapValue: { entries } },
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
  };

  return CallFunctionArgs.encode(args).finish();
}

export { serializeValue };
