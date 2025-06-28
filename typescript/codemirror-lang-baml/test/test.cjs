const { fileTests } = require('@lezer/generator/test')
const fs = require('fs')
const path = require('path')

// Import the CommonJS build
const { BAMLLanguage } = require('../dist/index.cjs')

// Find all test case files
const caseDir = path.join(__dirname, 'cases')
const testFiles = fs
  .readdirSync(caseDir)
  .filter((f) => f.endsWith('.txt'))
  .map((f) => path.join(caseDir, f))

// Run tests for each file
describe('BAML Grammar Tests', () => {
  for (const file of testFiles) {
    const name = path.basename(file, '.txt')
    describe(name, () => {
      for (const { name, run } of fileTests(fs.readFileSync(file, 'utf8'), file))
        it(name, () => run(BAMLLanguage.parser))
    })
  }
})
