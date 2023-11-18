export interface ParserDatabase {
  functions: SFunction[]
}

interface StringSpan {
  value: string
  start: number
  end: number
  source_file: string
}

export type ArgType =
  | {
    arg_type: 'positional'
    type: string
  }
  | {
    arg_type: 'named'
    values: {
      name: StringSpan
      type: string
    }[]
  }

interface Impl {
  type: 'llm'
  name: StringSpan
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
}
