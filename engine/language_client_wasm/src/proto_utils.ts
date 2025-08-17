import { CFFIValueHolder } from './proto/cffi_pb.js';
import { encodeValue } from './encode.js';

export function serializeArgs(args: Record<string, any>): Uint8Array {
  const holder = new CFFIValueHolder();
  holder.value = {
    case: 'classValue',
    value: {
      name: 'Arguments',
      fields: Object.fromEntries(
        Object.entries(args).map(([k, v]) => [k, encodeValue(v)])
      )
    }
  };
  
  // Use actual protobuf serialization
  return CFFIValueHolder.toBinary(holder);
}

export function deserializeResult(data: Uint8Array): CFFIValueHolder {
  return CFFIValueHolder.fromBinary(data);
}