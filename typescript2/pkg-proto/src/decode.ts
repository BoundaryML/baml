import type {
  BamlOutboundValue as BamlOutboundValueType,
  BamlOutboundMapEntry,
  BamlValueMedia,
  BamlValuePromptAst,
  BamlValuePromptAstSimple,
} from './generated/baml_bridge/cffi/v1/baml_outbound';
import { BamlOutboundValue, MediaTypeEnum } from './generated/baml_bridge/cffi/v1/baml_outbound';
import { BamlHandleType } from './generated/baml_bridge/cffi/v1/baml_handle';
import type { BamlJsValue, BamlJsClass, BamlJsHandle, BamlJsMedia, BamlJsPromptAst, BamlJsPromptAstSimple, BamlJsPromptAstMessage } from './types';

const HANDLE_TYPE_NAMES: Record<number, string> = {
  [BamlHandleType.HANDLE_UNSPECIFIED]: 'unspecified',
  [BamlHandleType.UNTAGGED_RUST_DATA]: 'rust_data',
  [BamlHandleType.UNTAGGED_BEX_HEAP]: 'bex_heap',
  [BamlHandleType.FUNCTION_REF]: 'function_ref',
  [BamlHandleType.ADT_MEDIA_IMAGE]: 'image',
  [BamlHandleType.ADT_MEDIA_AUDIO]: 'audio',
  [BamlHandleType.ADT_MEDIA_VIDEO]: 'video',
  [BamlHandleType.ADT_MEDIA_PDF]: 'pdf',
  [BamlHandleType.ADT_MEDIA_GENERIC]: 'media',
  [BamlHandleType.ADT_PROMPT_AST]: 'prompt_ast',
  [BamlHandleType.ADT_COLLECTOR]: 'collector',
  [BamlHandleType.ADT_TYPE]: 'type',
};

export function handleTypeName(handleType: number): string {
  return HANDLE_TYPE_NAMES[handleType] ?? `handle(${handleType})`;
}

export type WrapHandleFn<T> = (key: bigint, handleType: number, typeName: string) => T;

/**
 * Decode a base-sixteen hex string (the wire format for bigint values and
 * literals, produced by Rust's `format!("{:x}")`) into a `bigint`. A leading
 * `-` denotes a negative value; there is no `0x` prefix on the wire.
 *
 * Guards against empty or sign-only input: `BigInt('0x')` throws an opaque
 * `SyntaxError`, so we surface a clearer error for malformed wire data.
 */
// Workspace bigint cap = 2^28 bits ⇒ at most (2^28)/4 hex digits (plus
// slack), matching the Rust-side `MAX_BIGINT_HEX_LEN` in
// `bridge_ctypes/src/value_decode.rs`. Reject longer inputs before
// calling `BigInt()` so a malicious payload can't drive an unbounded
// allocation on the JS heap.
const MAX_BIGINT_HEX_LEN = (1 << 28) / 4 + 2;
function hexToBigInt(hex: string): bigint {
  const negative = hex.startsWith('-');
  const magnitude = negative ? hex.slice(1) : hex;
  if (magnitude.length === 0 || !/^[0-9a-fA-F]+$/.test(magnitude)) {
    throw new Error(`Invalid bigint hex on the wire: ${JSON.stringify(hex)}`);
  }
  if (magnitude.length > MAX_BIGINT_HEX_LEN) {
    throw new Error(
      `bigint hex exceeds the workspace cap (${magnitude.length} chars, limit ${MAX_BIGINT_HEX_LEN})`,
    );
  }
  const value = BigInt(`0x${magnitude}`);
  return negative ? -value : value;
}

const MEDIA_TYPE_NAMES: Record<number, BamlJsMedia['media_type']> = {
  [MediaTypeEnum.MEDIA_TYPE_UNSPECIFIED]: 'other',
  [MediaTypeEnum.IMAGE]: 'image',
  [MediaTypeEnum.AUDIO]: 'audio',
  [MediaTypeEnum.PDF]: 'pdf',
  [MediaTypeEnum.VIDEO]: 'video',
  [MediaTypeEnum.OTHER]: 'other',
};

function mediaTypeName(mt: MediaTypeEnum): BamlJsMedia['media_type'] {
  return MEDIA_TYPE_NAMES[mt] ?? 'other';
}

function tryParseJson(s: string): unknown {
  try {
    return JSON.parse(s);
  } catch {
    return s;
  }
}

function deserializeMedia(m: BamlValueMedia): BamlJsMedia {
  const base = {
    $baml: { type: '$media' as const },
    media_type: mediaTypeName(m.media),
    ...(m.mimeType != null ? { mime_type: m.mimeType } : {}),
  };
  if (!m.value) return { ...base, content_type: 'url' as const, url: '' };
  switch (m.value.$case) {
    case 'url':
      return { ...base, content_type: 'url' as const, url: m.value.url };
    case 'base64':
      return { ...base, content_type: 'base64' as const, base64: m.value.base64 };
    case 'file':
      return { ...base, content_type: 'file' as const, file: m.value.file };
    default: {
      const _exhaustive: never = m.value;
      return { ...base, content_type: 'url' as const, url: '' };
    }
  }
}

function deserializePromptAstSimple(s: BamlValuePromptAstSimple): BamlJsPromptAstSimple {
  if (!s.value) return { $baml: { type: '$prompt_ast_simple' }, content_type: 'string', value: '' };
  switch (s.value.$case) {
    case 'string':
      return { $baml: { type: '$prompt_ast_simple' }, content_type: 'string', value: s.value.string };
    case 'media':
      return { $baml: { type: '$prompt_ast_simple' }, content_type: 'media', value: deserializeMedia(s.value.media) };
    case 'multiple':
      return { $baml: { type: '$prompt_ast_simple' }, content_type: 'multiple', value: s.value.multiple.items.map(deserializePromptAstSimple) };
    default: {
      const _exhaustive: never = s.value;
      return { $baml: { type: '$prompt_ast_simple' }, content_type: 'string', value: '' };
    }
  }
}

function deserializePromptAst(ast: BamlValuePromptAst): BamlJsPromptAst {
  if (!ast.value) return { $baml: { type: '$prompt_ast' }, content_type: 'simple', value: { $baml: { type: '$prompt_ast_simple' }, content_type: 'string', value: '' } };
  switch (ast.value.$case) {
    case 'simple':
      return { $baml: { type: '$prompt_ast' }, content_type: 'simple', value: deserializePromptAstSimple(ast.value.simple) };
    case 'message': {
      const msg = ast.value.message;
      const message: BamlJsPromptAstMessage = {
        $baml: { type: '$prompt_ast_message' },
        role: msg.role,
        content: msg.content ? deserializePromptAstSimple(msg.content) : null,
        ...(msg.metadataAsJson ? { metadata: tryParseJson(msg.metadataAsJson) } : {}),
      };
      return { $baml: { type: '$prompt_ast' }, content_type: 'message', value: message };
    }
    case 'multiple':
      return { $baml: { type: '$prompt_ast' }, content_type: 'multiple', value: ast.value.multiple.items.map(deserializePromptAst) };
    default: {
      const _exhaustive: never = ast.value;
      return { $baml: { type: '$prompt_ast' }, content_type: 'simple', value: { $baml: { type: '$prompt_ast_simple' }, content_type: 'string', value: '' } };
    }
  }
}

function deserializeMapEntries<T>(
  entries: BamlOutboundMapEntry[],
  wrapHandle: WrapHandleFn<T>,
): Record<string, BamlJsValue<T>> {
  const result: Record<string, BamlJsValue<T>> = {};
  for (const entry of entries) {
    result[entry.key] = entry.value
      ? deserializeValue(entry.value, wrapHandle)
      : null;
  }
  return result;
}

function deserializeValue<T>(
  holder: BamlOutboundValueType,
  wrapHandle: WrapHandleFn<T>,
): BamlJsValue<T> {
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
      const fields = deserializeMapEntries(cls.fields, wrapHandle);
      return {
        $baml: { type: cls.name ?? '' },
        ...fields,
      } as BamlJsClass<T>;
    }

    case 'enumValue':
      return holder.value.enumValue.value;

    case 'listValue':
      return holder.value.listValue.items.map((item) => deserializeValue(item, wrapHandle));

    case 'mapValue':
      return deserializeMapEntries(holder.value.mapValue.entries, wrapHandle);

    case 'literalValue': {
      const lit = holder.value.literalValue;
      if (!lit.literal) return null;
      switch (lit.literal.$case) {
        case 'stringValue':
          return lit.literal.stringValue;
        case 'intValue':
          return lit.literal.intValue;
        case 'boolValue':
          return lit.literal.boolValue;
        case 'bigintValue':
          return hexToBigInt(lit.literal.bigintValue);
        case 'floatValue':
          return Number(lit.literal.floatValue);
        default: {
          const _exhaustive: never = lit.literal;
          return null;
        }
      }
    }

    case 'unionVariantValue':
      return holder.value.unionVariantValue.value
        ? deserializeValue(holder.value.unionVariantValue.value, wrapHandle)
        : null;

    case 'handleValue': {
      const handle = holder.value.handleValue;
      const key =
        typeof handle.key === 'bigint' ? handle.key : BigInt(handle.key ?? 0);
      return {
        $baml: { type: '$handle' as const },
        handle: wrapHandle(key, handle.handleType, handleTypeName(handle.handleType)),
      } satisfies BamlJsHandle<T>;
    }

    case 'mediaValue':
      return deserializeMedia(holder.value.mediaValue);

    case 'promptAstValue':
      return deserializePromptAst(holder.value.promptAstValue);

    case 'uint8arrayValue':
      return holder.value.uint8arrayValue;

    case 'bigintValue':
      return hexToBigInt(holder.value.bigintValue);

    default:
      return null;
  }
}

export function decodeCallResult<T>(
  bytes: Uint8Array,
  wrapHandle: WrapHandleFn<T>,
): BamlJsValue<T> {
  const holder = BamlOutboundValue.decode(bytes);
  return deserializeValue(holder, wrapHandle);
}

export { deserializeValue };
