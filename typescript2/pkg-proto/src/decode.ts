import type {
  BamlOutboundValue as BamlOutboundValueType,
  BamlOutboundMapEntry,
  BamlValueMedia,
  BamlValuePromptAst,
  BamlValuePromptAstSimple,
} from './generated/baml/cffi/v1/baml_outbound';
import { BamlOutboundValue, MediaTypeEnum } from './generated/baml/cffi/v1/baml_outbound';
import { BamlHandleType } from './generated/baml/cffi/v1/baml_inbound';
import type { BamlJsValue, BamlJsClass, BamlJsHandle, BamlJsMedia, BamlJsPromptAst, BamlJsPromptAstSimple, BamlJsPromptAstMessage } from './types';

const HANDLE_TYPE_NAMES: Record<number, string> = {
  [BamlHandleType.HANDLE_UNSPECIFIED]: 'unspecified',
  [BamlHandleType.HANDLE_UNKNOWN]: 'unknown',
  [BamlHandleType.RESOURCE_FILE]: 'file',
  [BamlHandleType.RESOURCE_SOCKET]: 'socket',
  [BamlHandleType.RESOURCE_HTTP_RESPONSE]: 'http_response',
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
        $baml: { type: cls.name?.name ?? '' },
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
        ? deserializeValue(holder.value.unionVariantValue.value, wrapHandle)
        : null;

    case 'checkedValue':
      return holder.value.checkedValue.value
        ? deserializeValue(holder.value.checkedValue.value, wrapHandle)
        : null;

    case 'streamingStateValue':
      return holder.value.streamingStateValue.value
        ? deserializeValue(holder.value.streamingStateValue.value, wrapHandle)
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
