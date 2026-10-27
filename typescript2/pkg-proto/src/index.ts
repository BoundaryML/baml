// High-level API

export type { WrapHandleFn } from './decode';
export { decodeCallResult, deserializeValue, handleTypeName } from './decode';
// Proto types (for .toBaml() implementors)
export type {
  CallFunctionArgs,
  InboundClassValue,
  InboundEnumValue,
  InboundListValue,
  InboundMapEntry,
  InboundMapValue,
  InboundValue,
} from './generated/baml_bridge/cffi/v1/baml_inbound';
export { BamlOutboundValue } from './generated/baml_bridge/cffi/v1/baml_outbound';
export type {
  BamlJsClass,
  BamlJsHandle,
  BamlJsMap,
  BamlJsMedia,
  BamlJsPromptAst,
  BamlJsPromptAstMessage,
  BamlJsPromptAstSimple,
  BamlJsValue,
  PlainHandleDescriptor,
} from './types';
