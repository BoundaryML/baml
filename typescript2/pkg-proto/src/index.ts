// High-level API
export { encodeCallArgs, serializeValue } from './encode';
export type { EncodeCallArgsOptions } from './encode';
export { decodeCallResult, deserializeValue } from './decode';
export type { BamlJsValue, BamlJsClass, BamlJsMap, BamlSerializable } from './types';

// Proto types (for .toBaml() implementors)
export type {
  HostValue,
  HostClassValue,
  HostEnumValue,
  HostMapEntry,
  HostListValue,
  HostMapValue,
  HostFunctionArguments,
} from './generated/baml/cffi/v1/baml_inbound';
export type { CFFIValueHolder } from './generated/baml/cffi/v1/baml_outbound';
