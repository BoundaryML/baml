import type {
  HostValue,
  HostMapEntry,
  HostFunctionArguments as HostFunctionArgumentsType,
} from './generated/baml/cffi/v1/baml_inbound';
import {
  HostFunctionArguments,
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

function serializeValue(val: unknown): HostValue {
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
    const entries: HostMapEntry[] = Object.entries(val).map(
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

export interface EncodeCallArgsOptions {
  env?: Record<string, string>;
}

export function encodeCallArgs(
  kwargs: Record<string, unknown>,
  options?: EncodeCallArgsOptions,
): Uint8Array {
  const entries: HostMapEntry[] = Object.entries(kwargs).map(
    ([k, v]) => ({
      key: { $case: 'stringKey' as const, stringKey: k },
      value: serializeValue(v),
    }),
  );

  const args: HostFunctionArgumentsType = {
    kwargs: entries,
    clientRegistry: undefined,
    env: Object.entries(options?.env ?? {}).map(([key, value]) => ({
      key,
      value,
    })),
    collectors: [],
    typeBuilder: undefined,
    tags: [],
  };

  return HostFunctionArguments.encode(args).finish();
}

export { serializeValue };
