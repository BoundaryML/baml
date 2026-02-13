import type { HostValue } from './generated/baml/cffi/v1/baml_inbound';

export type BamlJsValue =
  | string
  | number
  | boolean
  | null
  | BamlJsValue[]
  | BamlJsMap
  | BamlJsClass;

export type BamlJsMap = { [key: string]: BamlJsValue };
export type BamlJsClass = { $baml: { type: string }; [key: string]: BamlJsValue };

/** Implemented by objects that need custom BAML serialization. */
export interface BamlSerializable {
  toBaml(): HostValue;
}
