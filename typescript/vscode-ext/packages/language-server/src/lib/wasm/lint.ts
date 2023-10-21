import { languageWasm } from '.'
import { handleFormatPanic, handleWasmError } from './internals'

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
  source_file: string
}

export interface LinterSourceFile {
  path: string;
  content: string;
}

export interface LinterInput {
  root_path: string
  files: LinterSourceFile[]
}

export default function lint(input: LinterInput, onError?: (errorMessage: string) => void): LinterError[] {
  console.log('running lint() from baml-schema-wasm')
  try {
    if (process.env.FORCE_PANIC_baml_SCHEMA) {
      handleFormatPanic(() => {
        console.debug('Triggering a Rust panic...')
        languageWasm.debug_panic()
      })
    }
    console.log("lint input " + JSON.stringify(input, null, 2));

    const result = languageWasm.lint(JSON.stringify(input));
    console.log("lint result " + JSON.stringify(result, null, 2));

    return JSON.parse(result) as LinterError[]
  } catch (e) {
    const err = e as Error

    handleWasmError(err, 'lint', onError)

    return []
  }
}
