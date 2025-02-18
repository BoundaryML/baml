#!/usr/bin/env node

if (require.main === module) {
  if (!process.env.BAML_LOG) {
    process.env.BAML_LOG = 'info'
  }

  const baml = require('./native')
  baml.invoke_runtime_cli(process.argv.slice(1))
}
