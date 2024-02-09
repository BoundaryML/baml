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

export type Impl = {
  type: 'llm'
  name: StringSpan
  prompt_key: Span
  input_replacers: { key: string; value: string }[]
  output_replacers: { key: string; value: string }[]
  client: StringSpan
} & (
    {
      has_v2?: false
      prompt: string
    } | {
      has_v2: true
      prompt_v2: {
        is_chat: false,
        prompt: string
      } | {
        is_chat: true,
        prompt: {
          role: string
          content: string
        }[]
      }
    }
  )

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
