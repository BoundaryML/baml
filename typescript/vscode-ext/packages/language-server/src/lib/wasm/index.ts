import languageWasm from '@gloo-ai/baml-schema-wasm'

// This is set in launch.json
if (process.env.VSCODE_DEBUG_MODE === 'true') {
  languageWasm.enable_logs();
}

export { languageWasm }
