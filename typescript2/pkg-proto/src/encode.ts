import type {
  InboundValue,
  InboundMapEntry,
  CallFunctionArgs as CallFunctionArgsType,
} from './generated/baml_core/cffi/v1/baml_inbound';
import {
  CallFunctionArgs,
} from './generated/baml_core/cffi/v1/baml_inbound';
import type { BamlSerializable } from './types';

function deserializeInboundValue(value: InboundValue): unknown {
  const v = value.value;
  if (!v) return null;
  switch (v.$case) {
    case 'stringValue': return v.stringValue;
    case 'intValue': return v.intValue;
    case 'floatValue': return v.floatValue;
    case 'boolValue': return v.boolValue;
    case 'listValue': return v.listValue.values.map(deserializeInboundValue);
    case 'mapValue': {
      const obj: Record<string, unknown> = {};
      for (const entry of v.mapValue.entries) {
        const key = entry.key;
        if (!key) continue;
        const keyStr =
          key.$case === 'stringKey' ? key.stringKey :
          key.$case === 'intKey' ? String(key.intKey) :
          key.$case === 'boolKey' ? String(key.boolKey) :
          key.$case === 'enumKey' ? key.enumKey.value :
          '';
        obj[keyStr] = entry.value ? deserializeInboundValue(entry.value) : null;
      }
      return obj;
    }
    case 'classValue': {
      const obj: Record<string, unknown> = {};
      for (const entry of v.classValue.fields) {
        if (entry.key?.$case !== 'stringKey') continue;
        obj[entry.key.stringKey] = entry.value ? deserializeInboundValue(entry.value) : null;
      }
      return obj;
    }
    case 'enumValue': return v.enumValue.value;
    case 'handle': return null;
    case 'uint8arrayValue': return null;
    default: return null;
  }
}

export function decodeCallArgs(bytes: Uint8Array): Record<string, unknown> {
  const args = CallFunctionArgs.decode(bytes);
  const obj: Record<string, unknown> = {};
  for (const entry of args.kwargs) {
    if (entry.key?.$case !== 'stringKey') continue;
    obj[entry.key.stringKey] = entry.value ? deserializeInboundValue(entry.value) : null;
  }
  return obj;
}

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
): Uint8Array {
  const entries: InboundMapEntry[] = Object.entries(kwargs).map(
    ([k, v]) => ({
      key: { $case: 'stringKey' as const, stringKey: k },
      value: serializeValue(v),
    }),
  );

  const args: CallFunctionArgsType = {
    kwargs: entries,
  };

  return CallFunctionArgs.encode(args).finish();
}

export { serializeValue };
