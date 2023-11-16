import { languageWasm } from '.'
import { handleFormatPanic, handleWasmError } from './internals'

type LintResponse = {
  diagnostics: LinterError[]
} & (
  | { ok: false }
  | {
      ok: true
      response: ParserDatabase
    }
)

export interface ParserDatabase {
  functions: SFunction[]
}

interface StringSpan {
  value: string
  start: number
  end: number
  source_file: string
}

type ArgType =
  | {
      arg_type: 'positional'
      type: string
    }
  | {
      arg_type: 'named'
      values: {
        name: string
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

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
  source_file: string
}

export interface LinterSourceFile {
  path: string
  content: string
}

export interface LinterInput {
  root_path: string
  files: LinterSourceFile[]
}

export default function lint(input: LinterInput, onError?: (errorMessage: string) => void): LintResponse {
  console.log('running lint() from baml-schema-wasm')
  try {
    if (process.env.FORCE_PANIC_baml_SCHEMA) {
      handleFormatPanic(() => {
        console.debug('Triggering a Rust panic...')
        languageWasm.debug_panic()
      })
    }

    const result = languageWasm.lint(JSON.stringify(input))
    const parsed = JSON.parse(result) as LintResponse
    console.log(`lint result ${JSON.stringify(JSON.parse(result), null, 2)}`)
    return parsed
  } catch (e) {
    const err = e as Error

    handleWasmError(err, 'lint', onError)

    return {
      ok: false,
      diagnostics: [],
    }
  }
}
