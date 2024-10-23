export {
  BamlRuntime,
  FunctionResult,
  FunctionResultStream,
  BamlImage as Image,
  ClientBuilder,
  BamlAudio as Audio,
  invoke_runtime_cli,
  ClientRegistry,
  BamlLogEvent,
} from './native'
export { BamlStream } from './stream'
export { BamlCtxManager } from './async_context_vars'
export { Checked } from './checked'

export class BamlValidationError extends Error {
  prompt: string
  raw_output: string

  constructor(prompt: string, raw_output: string, message: string) {
    super(message)
    this.name = 'BamlValidationError'
    this.prompt = prompt
    this.raw_output = raw_output

    Object.setPrototypeOf(this, BamlValidationError.prototype)
  }

  static from(error: Error): BamlValidationError | Error {
    if (error.message.includes('BamlValidationError')) {
      try {
        const errorData = JSON.parse(error.message)
        if (errorData.type === 'BamlValidationError') {
          return new BamlValidationError(
            errorData.prompt || '',
            errorData.raw_output || '',
            errorData.message || error.message,
          )
        } else {
          console.warn('Not a BamlValidationError:', error)
        }
      } catch (parseError) {
        // If JSON parsing fails, fall back to the original error
        console.warn('Failed to parse BamlValidationError:', parseError)
      }
    }

    // If it's not a BamlValidationError or parsing failed, return the original error
    return error
  }

  toJSON(): string {
    return JSON.stringify(
      {
        message: this.message,
        raw_output: this.raw_output,
        prompt: this.prompt,
      },
      null,
      2,
    )
  }
}

// Helper function to safely create a BamlValidationError
export function createBamlValidationError(error: Error): BamlValidationError | Error {
  return BamlValidationError.from(error)
}

// No need for a separate throwBamlValidationError function in TypeScript
