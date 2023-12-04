export interface ParserDatabase {
  functions: SFunction[]
  classes: SClass[]
  enums: SEnum[]
  clients: SClient[]
}

interface SClient {
  name: StringSpan
}

interface SClass {
  name: StringSpan
  jsonSchema: Record<string, any>
}

interface SEnum {
  name: StringSpan
  jsonSchema: Record<string, any>
}

interface Span {
  start: number
  end: number
  source_file: string
}

export interface StringSpan extends Span {
  value: string
}

export type ArgType =
  | {
      arg_type: 'positional'
      type: string
      jsonSchema: Record<string, any>
    }
  | {
      arg_type: 'named'
      values: {
        name: StringSpan
        type: string
        jsonSchema: Record<string, any>
      }[]
    }

interface Impl {
  type: 'llm'
  name: StringSpan
  prompt_key: Span
  prompt: string
  input_replacers: { key: string; value: string }[]
  output_replacers: { key: string; value: string }[]
  client: StringSpan
}

interface SFunction {
  name: StringSpan
  input: ArgType
  output: ArgType
  impls: Impl[]
  test_cases: {
    name: StringSpan
    // For now this is json.
    content: string
  }[]
}
