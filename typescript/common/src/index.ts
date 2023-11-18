import { ParserDatabase, ArgType } from './parser_db'

export interface TestRequest {
  functions: {
    name: string
    // TODO: Remove.
    input_type: string
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

export { ParserDatabase }
