import baml_ts from './index'
;(async () => {
  const b = baml_ts.BamlRuntimeFfi.fromDirectory('/home/sam/baml/integ-tests/baml_src')

  const result = await b.callFunction('ExtractNames', { input: 'hello this is patrick' }, {})

  console.log(result)
})()
