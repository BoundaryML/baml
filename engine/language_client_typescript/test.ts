import { BamlRuntimeFfi, rustIsInstance } from './index'
import { BamlClient } from './integ-tests-ts-v2/client'
;(async () => {
  const x = BamlRuntimeFfi.fromDirectory('/home/sam/baml/integ-tests/baml_src')
  console.log('rust-based isinstance ', rustIsInstance(x))

  const b = BamlClient.fromDirectory('/home/sam/baml/integ-tests/baml_src')

  const result = await b.ExtractNames({ input: 'hello this is patrick' })

  console.log('llm result from lang client typescript: ' + JSON.stringify(result, null, 2))
})()
