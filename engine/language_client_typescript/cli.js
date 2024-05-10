if (require.main === module) {
  const baml = require('@boundaryml/baml')
  baml.BamlRuntimeFfi.runCli(process.argv.slice(1))
}
