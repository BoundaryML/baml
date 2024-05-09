import { BamlRuntimeFfi } from './index'
;(async () => {
  //const content = await readFileAsync('Cargo.toml')

  const b = BamlRuntimeFfi.fromDirectory('/home/sam/baml/integ-tests/baml_src')

  const result = await b.call_function('ExtractNames', { input: 'hello this is patrick' }, {})

  console.log(result)
})()
