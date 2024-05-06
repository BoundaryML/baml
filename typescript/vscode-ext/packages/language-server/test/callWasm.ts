import languageWasm from '@gloo-ai/baml-schema-wasm-node'

function callWasm() {
  const res = languageWasm.lint('test')
  console.log('res', res)
}

console.log('calling wasm')
callWasm()
