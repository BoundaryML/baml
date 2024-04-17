export interface ParserDatabase {
  functions: SFunction[]
  classes: SClass[]
  enums: SEnum[]
  clients: SClient[]
}

export interface SClient {
  name: StringSpan
}

export interface SClass {
  name: StringSpan
  jsonSchema: Record<string, any>
}

export interface SEnum {
  name: StringSpan
  jsonSchema: Record<string, any>
}

export interface Span {
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
  client: {
    identifier: StringSpan
    provider: string
    model?: string
  }
  prompt: {
    test_case?: string;
  } & ({
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
  })
  input_replacers: [string, string][],
  output_replacers: [string, string][],
};

export interface SFunction {
  name: StringSpan
  input: ArgType
  output: ArgType
  impls: Impl[]
  test_cases: {
    name: StringSpan
    // For now this is a JSON.parse-able string
    content: string
  }[]
  syntax: "Version1" | "Version2"
}
