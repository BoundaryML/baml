// High-level API
export { encodeCallArgs, serializeValue } from './encode';
export { decodeCallResult, deserializeValue, handleTypeName } from './decode';
export type { WrapHandleFn } from './decode';
export type { BamlJsValue, BamlJsClass, BamlJsMap, BamlJsHandle, BamlJsMedia, BamlJsPromptAst, BamlJsPromptAstSimple, BamlJsPromptAstMessage, BamlSerializable, PlainHandleDescriptor } from './types';

// Proto types (for .toBaml() implementors)
export type {
  InboundValue,
  InboundClassValue,
  InboundEnumValue,
  InboundMapEntry,
  InboundListValue,
  InboundMapValue,
  CallFunctionArgs,
} from './generated/baml_core/cffi/v1/baml_inbound';
export type { BamlOutboundValue } from './generated/baml_core/cffi/v1/baml_outbound';

// Event types (for runtime observability)
export {
  RuntimeEvent,
  type EventKind,
  type FunctionStartEvent,
  type FunctionEndEvent,
  type SetTagsEvent,
  type LogEvent,
  type CustomEvent,
  type TagEntry,
  type SourceLocation,
} from './generated/baml_core/cffi/v1/baml_events';
