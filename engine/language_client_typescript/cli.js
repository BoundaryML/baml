#!/usr/bin/env node

if (require.main === module) {
  const baml = require('./native')
  baml.invoke_runtime_cli(process.argv.slice(1))
}
