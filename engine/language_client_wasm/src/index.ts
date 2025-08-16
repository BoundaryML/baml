export { BamlWasmRuntime } from './runtime.js';
export { BamlSerializer, encodeValue } from './encode.js';
export { decodeValue, TypeMap } from './decode.js';
export type { CFFIValueHolder } from './proto/cffi_pb.js';

// Browser environment check
if (typeof window === 'undefined') {
  throw new Error(
    'BAML WASM client is only supported in browser environments. ' +
    'For Node.js, use the native @boundaryml/baml package.'
  );
}