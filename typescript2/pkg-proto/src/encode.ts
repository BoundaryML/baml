import type {
  InboundValue,
  InboundMapEntry,
  CallFunctionArgs as CallFunctionArgsType,
} from './generated/baml/cffi/v1/baml_inbound';
import {
  CallFunctionArgs,
} from './generated/baml/cffi/v1/baml_inbound';
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
