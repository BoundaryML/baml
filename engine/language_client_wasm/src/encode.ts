import { CFFIValueHolder, CFFIFieldTypeHolder } from './proto/cffi_pb.js';

export interface BamlSerializer {
  encode(): CFFIValueHolder;
  bamlTypeName(): string;
  bamlEncodeName(): CFFIFieldTypeHolder;
}

export function encodeValue(value: any): CFFIValueHolder {
  const holder = new CFFIValueHolder();
  
  // Handle null
  if (value === null || value === undefined) {
    holder.value = { case: 'nullValue', value: {} };
    return holder;
  }
  
  // Handle primitives
  if (typeof value === 'string') {
    holder.value = { case: 'stringValue', value };
    return holder;
  }
  
  if (typeof value === 'number') {
    if (Number.isInteger(value)) {
      holder.value = { case: 'intValue', value: BigInt(value) };
    } else {
      holder.value = { case: 'floatValue', value };
    }
    return holder;
  }
  
  if (typeof value === 'boolean') {
    holder.value = { case: 'boolValue', value };
    return holder;
  }
  
  // Handle arrays
  if (Array.isArray(value)) {
    const items = value.map(item => encodeValue(item));
    holder.value = { case: 'listValue', value: { items } };
    return holder;
  }
  
  // Handle objects/classes
  if (typeof value === 'object') {
    // Check if it implements BamlSerializer
    if ('encode' in value && typeof value.encode === 'function') {
      return value.encode();
    }
    
    // Generic object encoding
    const fields: Record<string, CFFIValueHolder> = {};
    for (const [key, val] of Object.entries(value)) {
      fields[key] = encodeValue(val);
    }
    holder.value = { case: 'classValue', value: { name: 'DynamicClass', fields } };
    return holder;
  }
  
  throw new Error(`Cannot encode value of type: ${typeof value}`);
}