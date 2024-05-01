import { ParserDatabase } from '@baml/common'
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
  // Function Name -> Test Name
  selected_tests: Record<string, string>
}

export default function lint(input: LinterInput, onError?: (errorMessage: string) => void): LintResponse {
  try {
    if (process.env.FORCE_PANIC_baml_SCHEMA) {
      handleFormatPanic(() => {
        console.debug('Triggering a Rust panic...')
        languageWasm.debug_panic()
      })
    }

    let res = languageWasm.create_runtime(input.root_path, input.files);
    let func = res.get_function("ExtractResume2");
    let ctx = new languageWasm.WasmRuntimeContext();
    let prompt = func?.render_prompt(res, ctx, { resume: "Hi! I'm johsn!" });

    console.log(`prompt: ${prompt?.as_chat()?.map((c) => `${c.role}: ${c.parts.length} Parts`).join("\n")}`);

    throw new Error('Not implemented')

    const result = languageWasm.lint(JSON.stringify(input))
    const parsed = JSON.parse(result) as LintResponse
    // console.log(`lint result ${JSON.stringify(JSON.parse(result), null, 2)}`)
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
