import type {
  CFFIValueHolder as CFFIValueHolderType,
  CFFIMapEntry,
} from './generated/baml/cffi/v1/baml_outbound';
import { CFFIValueHolder } from './generated/baml/cffi/v1/baml_outbound';
import type { BamlJsValue, BamlJsClass } from './types';

function deserializeMapEntries(
  entries: CFFIMapEntry[],
): Record<string, BamlJsValue> {
  const result: Record<string, BamlJsValue> = {};
  for (const entry of entries) {
    result[entry.key] = entry.value
      ? deserializeValue(entry.value)
      : null;
  }
  return result;
}

function deserializeValue(holder: CFFIValueHolderType): BamlJsValue {
  if (!holder.value) return null;

  switch (holder.value.$case) {
    case 'nullValue':
      return null;

    case 'stringValue':
      return holder.value.stringValue;

    case 'intValue':
      return holder.value.intValue;

    case 'floatValue':
      return holder.value.floatValue;

    case 'boolValue':
      return holder.value.boolValue;

    case 'classValue': {
      const cls = holder.value.classValue;
      const fields = deserializeMapEntries(cls.fields);
      return {
        $baml: { type: cls.name?.name ?? '' },
        ...fields,
      } as BamlJsClass;
    }

    case 'enumValue':
      return holder.value.enumValue.value;

    case 'listValue':
      return holder.value.listValue.items.map(deserializeValue);

    case 'mapValue':
      return deserializeMapEntries(holder.value.mapValue.entries);

    case 'literalValue': {
      const lit = holder.value.literalValue;
      if (!lit.literal) return null;
      switch (lit.literal.$case) {
        case 'stringLiteral':
          return lit.literal.stringLiteral.value;
        case 'intLiteral':
          return lit.literal.intLiteral.value;
        case 'boolLiteral':
          return lit.literal.boolLiteral.value;
        default: {
          const _exhaustive: never = lit.literal;
          return null;
        }
      }
    }

    case 'unionVariantValue':
      return holder.value.unionVariantValue.value
        ? deserializeValue(holder.value.unionVariantValue.value)
        : null;

    case 'checkedValue':
      return holder.value.checkedValue.value
        ? deserializeValue(holder.value.checkedValue.value)
        : null;

    case 'streamingStateValue':
      return holder.value.streamingStateValue.value
        ? deserializeValue(holder.value.streamingStateValue.value)
        : null;

    case 'objectValue':
      // Raw object handles are not representable in JS
      return null;

    default:
      return null;
  }
}

export function decodeCallResult(bytes: Uint8Array): BamlJsValue {
  const holder = CFFIValueHolder.decode(bytes);
  return deserializeValue(holder);
}

export { deserializeValue };
