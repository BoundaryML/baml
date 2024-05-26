if (require.main === module) {
  const baml = require('@boundaryml/baml')
  baml.invoke_runtime_cli(process.argv.slice(1))
}
