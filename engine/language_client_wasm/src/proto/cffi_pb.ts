// Placeholder TypeScript protobuf types
// This file will be auto-generated when building with `cargo build --features wasm`

export class CFFIValueHolder {
  type?: CFFIFieldTypeHolder;
  value?: {
    case: 'nullValue' | 'stringValue' | 'intValue' | 'floatValue' | 'boolValue' | 'listValue' | 'mapValue' | 'classValue';
    value: any;
  };

  constructor() {
    this.value = undefined;
  }
}

export class CFFIFieldTypeHolder {
  type?: any;
}

export interface CFFIValueList {
  items: CFFIValueHolder[];
}

export interface CFFIValueMap {
  entries: Record<string, CFFIValueHolder>;
}

export interface CFFIValueClass {
  name: string;
  fields: Record<string, CFFIValueHolder>;
}