import { languageWasm } from '.'
import { handleFormatPanic, handleWasmError } from './internals'
import { TestRequest } from '@baml/common'

export interface LinterSourceFile {
  path: string
  content: string
}

export interface GenerateInput {
  root_path: string
  files: LinterSourceFile[]
  test_request: TestRequest
}

export type GenerateResponse =
  | {
    status: 'error'
    message: string
  }
  | {
    status: 'ok'
    content: string
  }

export default function generate_test_file(
  input: GenerateInput,
  onError?: (errorMessage: string) => void,
): GenerateResponse {
  console.log('running generate_test_file() from baml-schema-wasm')
  try {
    if (process.env.FORCE_PANIC_baml_SCHEMA) {
      handleFormatPanic(() => {
        console.debug('Triggering a Rust panic...')
        languageWasm.debug_panic()
      })
    }
    console.log(`generate input ${JSON.stringify(input, null, 2)}`)

    const result = languageWasm.generate_test_file(JSON.stringify(input))
    const parsed = JSON.parse(result) as GenerateResponse
    console.log(`generate result ${JSON.stringify(JSON.parse(result), null, 2)}`)
    return parsed
  } catch (e) {
    const err = e as Error

    handleWasmError(err, 'lint', onError)

    return {
      status: 'error',
      message: err.message,
    }
  }
}
