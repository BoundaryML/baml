import * as BamlWasm from '@gloo-ai/baml-schema-wasm-node'

// This is set in launch.json
if (process.env.VSCODE_DEBUG_MODE === 'true') {
  BamlWasm.enable_logs();
}

export { BamlWasm }
