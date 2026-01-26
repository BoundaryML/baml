import path from 'path'
import { BamlRuntimeFfi, rustIsInstance } from './index'
import { b as BamlClient } from '../../integ-tests/typescript/baml_client/async_client'

;(async () => {
  const bamlSrc = path.resolve(__dirname, '../../integ-tests/baml_src')

  const x = BamlRuntimeFfi.fromDirectory(bamlSrc)
  console.log('rust-based isinstance ', rustIsInstance(x))

  const result = await BamlClient.ExtractNames({ input: 'hello this is patrick' })

  console.log('llm result from lang client typescript: ' + JSON.stringify(result, null, 2))
})()
