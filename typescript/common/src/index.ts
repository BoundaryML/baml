import { ParserDatabase, ArgType } from './parser_db'
export interface TestRequest {
  functions: {
    name: string
    tests: {
      name: string
      impls: string[]
      params:
      | {
        type: 'positional'
        value: string
      }
      | {
        type: 'named'
        value: {
          name: string
          value: string
        }[]
      }
    }[]
  }[]
}

export enum TestStatus {
  Queued = 'QUEUED',
  Running = 'RUNNING',
  Passed = 'PASSED',
  Failed = 'FAILED',
}


export interface TestResult {
  fullTestName: string;
  functionName: string;
  testName: string;
  implName: string;
  status: TestStatus;
  output: string;
}



function getFullTestName(testName: string, impl: string, fnName: string) {
  return `test_${testName}[${fnName}-${impl}]`
}

export { type ParserDatabase, getFullTestName }
export { clientEventLogSchema, type ClientEventLog } from './schemav2'
export { type StringSpan } from './parser_db'