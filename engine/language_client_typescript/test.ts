import { BamlClient } from './integ-tests-ts-v2/client'
;(async () => {
  const b = BamlClient.fromDirectory('/home/sam/baml/integ-tests/baml_src')

  const result = await b.ExtractNames({ input: 'hello this is patrick' })

  console.log('llm result from lang client typescript: ' + JSON.stringify(result, null, 2))
})()
