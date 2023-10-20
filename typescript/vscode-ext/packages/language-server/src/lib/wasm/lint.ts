import { languageWasm } from '.'
import { handleFormatPanic, handleWasmError } from './internals'

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
}

export default function lint(text: string, onError?: (errorMessage: string) => void): LinterError[] {
  console.log('running lint() from baml-schema-wasm')
  try {
    if (process.env.FORCE_PANIC_baml_SCHEMA) {
      handleFormatPanic(() => {
        console.debug('Triggering a Rust panic...')
        languageWasm.debug_panic()
      })
    }

    const result = languageWasm.lint(text)
    console.log("lint result", JSON.stringify(result, null, 2));

    return JSON.parse(result) as LinterError[]
  } catch (e) {
    const err = e as Error

    handleWasmError(err, 'lint', onError)

    return []
  }
}
