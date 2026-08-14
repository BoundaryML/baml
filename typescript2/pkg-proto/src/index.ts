// High-level API
export { encodeCallArgs, encodeRunArgs, serializeValue } from './encode';
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
} from './generated/baml_bridge/cffi/v1/baml_inbound';
export { BamlOutboundValue } from './generated/baml_bridge/cffi/v1/baml_outbound';
