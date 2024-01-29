import { ParserDatabase, ArgType } from './parser_db'
export interface TestRequest {
  functions: {
    name: string
    tests: {
      name: string
      impls: string[]
    }[]
  }[]
}

export interface TestFileContent {
  input: any
}

export enum TestStatus {
  Compiling = 'COMPILING',
  Queued = 'QUEUED',
  Running = 'RUNNING',
  Passed = 'PASSED',
  Failed = 'FAILED',
}

export interface TestResult {
  fullTestName: string
  functionName: string
  testName: string
  implName: string
  status: TestStatus
  url?: string
  input?: string
  output: {
    error?: string
    parsed?: string
    raw?: string
  }
}

export interface TestState {
  exit_code?: number
  results: TestResult[]
  test_url: string | null
  run_status: "NOT_STARTED" | "RUNNING" | "COMPLETED" | "ERROR"
}

function getFullTestName(testName: string, impl: string, fnName: string) {
  return `test_${testName}[${fnName}-${impl}]`
}

export { type ParserDatabase, getFullTestName }
export { clientEventLogSchema, type ClientEventLog } from './schemav2'
export { type StringSpan } from './parser_db'
