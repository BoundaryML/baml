
import {
  ParserDatabase,
  ArgType
} from "./parser_db";
export interface RunTestRequest {
  cases: TestCaseInfo[]
}

export interface TestCaseInfo {
  function_name: string
  input: TestInput
}

interface TestInput {
  argsInfo: ArgType
  // each element is a list of values for a positional argument
  values: any[]
}


export {
  ParserDatabase
}