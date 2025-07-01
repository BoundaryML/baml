import { fileTests } from '@lezer/generator/test'
import { parser } from '../src/syntax.grammar.js'
import * as fs from 'fs'
import * as path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

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
      for (const { name, run } of fileTests(fs.readFileSync(file, 'utf8'), file)) it(name, () => run(parser))
    })
  }
})
