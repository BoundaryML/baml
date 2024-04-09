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

// keep in sync with engine/baml-fmt/src/lint.rs
export type Impl = {
  type: 'llm'
  name: StringSpan
  prompt_key: Span
  client: StringSpan
  prompt: {
      type: "Completion"
      completion: string     
    } | {
      type: "Chat"
      chat: {
        role: string
        message: string
      }[]
    } | {
      type: "Error"
      error: string
    }
};

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
