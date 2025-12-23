// Re-export the WASM types and initialization
export { default as initWasm, WasmProject } from 'baml-playground-wasm';

// Re-export React components
export {
  BamlPlaygroundProvider,
  useWasmReady,
  useWasmError,
  useFunctions,
  useUpdateFile,
  useBamlPlayground,
} from './BamlPlaygroundProvider';
export { FunctionList } from './FunctionList';
export { BamlEditor } from './BamlEditor';

// Re-export atoms for direct access
export {
  wasmReadyAtom,
  wasmErrorAtom,
  projectAtom,
  functionsAtom,
  selectedFunctionAtom,
  filesAtom,
} from './atoms';
