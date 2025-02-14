// NOTE: Don't take a dependency on ./native here, it will break the browser code

export class BamlClientFinishReasonError extends Error {
  prompt: string
  raw_output: string
  finish_reason?: string

  constructor(prompt: string, raw_output: string, message: string, finish_reason: string | undefined) {
    super(message)
    this.name = 'BamlClientFinishReasonError'
    this.prompt = prompt
    this.raw_output = raw_output
    this.finish_reason = finish_reason

    Object.setPrototypeOf(this, BamlClientFinishReasonError.prototype)
  }

  toJSON(): string {
    return JSON.stringify(
      {
        name: this.name,
        message: this.message,
        raw_output: this.raw_output,
        prompt: this.prompt,
        finish_reason: this.finish_reason,
      },
      null,
      2,
    )
  }

  static from(error: Error): BamlClientFinishReasonError | undefined {
    if (error.message.includes('BamlClientFinishReasonError')) {
      try {
        const errorData = JSON.parse(error.message)
        if (errorData.type === 'BamlClientFinishReasonError') {
          return new BamlClientFinishReasonError(
            errorData.prompt || '',
            errorData.raw_output || '',
            errorData.message || error.message,
            errorData.finish_reason,
          )
        }
      } catch (parseError) {
        console.warn('Failed to parse BamlClientFinishReasonError:', parseError)
      }
    }
    return undefined
  }
}

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

  toJSON(): string {
    return JSON.stringify(
      {
        name: this.name,
        message: this.message,
        raw_output: this.raw_output,
        prompt: this.prompt,
      },
      null,
      2,
    )
  }

  static from(error: Error): BamlValidationError | undefined {
    if (error.message.includes('BamlValidationError')) {
      try {
        const errorData = JSON.parse(error.message)
        if (errorData.type === 'BamlValidationError') {
          return new BamlValidationError(
            errorData.prompt || '',
            errorData.raw_output || '',
            errorData.message || error.message,
          )
        }
      } catch (parseError) {
        console.warn('Failed to parse BamlValidationError:', parseError)
      }
    }
    return undefined
  }
}

export class BamlClientHttpError extends Error {
  client_name: string
  status_code: number

  constructor(client_name: string, message: string, status_code: number) {
    super(message)
    this.name = 'BamlClientHttpError'
    this.client_name = client_name
    this.status_code = status_code

    Object.setPrototypeOf(this, BamlClientHttpError.prototype)
  }

  toJSON(): string {
    return JSON.stringify({
      name: this.name,
      message: this.message,
      status_code: this.status_code,
      client_name: this.client_name,
    })
  }

  static from(error: Error): BamlClientHttpError | undefined {
    if (error.message.includes('BamlClientHttpError')) {
      try {
        const errorData = JSON.parse(error.message)
        if (errorData.type === 'BamlClientHttpError') {
          return new BamlClientHttpError(
            errorData.client_name || '',
            errorData.message || error.message,
            errorData.status_code || -100,
          )
        }
      } catch (parseError) {
        console.warn('Failed to parse BamlClientHttpError:', parseError)
      }
    }
    return undefined
  }
}

export type BamlErrors = BamlClientHttpError | BamlValidationError | BamlClientFinishReasonError

function isError(error: unknown): error is Error {
  if (typeof error === 'string') {
    return false
  }

  if ((error as any).message) {
    return true
  }

  if (error instanceof Error) {
    return true
  }

  return false
}

// Helper function to safely create a BamlValidationError
function createBamlErrorUnsafe(error: unknown): BamlErrors | Error {
  if (!isError(error)) {
    return new Error(String(error))
  }

  const bamlClientHttpError = BamlClientHttpError.from(error)
  if (bamlClientHttpError) {
    return bamlClientHttpError
  }

  const bamlValidationError = BamlValidationError.from(error)
  if (bamlValidationError) {
    return bamlValidationError
  }

  const bamlClientFinishReasonError = BamlClientFinishReasonError.from(error)
  if (bamlClientFinishReasonError) {
    return bamlClientFinishReasonError
  }

  // otherwise return the original error
  return error
}

export function isBamlError(error: unknown): error is BamlErrors {
  if (
    (error as any).type === 'BamlClientHttpError' ||
    (error as any).type === 'BamlValidationError' ||
    (error as any).type === 'BamlClientFinishReasonError'
  ) {
    return true
  }

  if (
    (error as any).name === 'BamlClientHttpError' ||
    (error as any).name === 'BamlValidationError' ||
    (error as any).name === 'BamlClientFinishReasonError'
  ) {
    return true
  }

  return (
    error instanceof BamlClientHttpError ||
    error instanceof BamlValidationError ||
    error instanceof BamlClientFinishReasonError
  )
}

export function toBamlError(error: unknown): BamlErrors | Error {
  try {
    if (isBamlError(error)) {
      return error
    }

    return createBamlErrorUnsafe(error)
  } catch (error) {
    return error as Error
  }
}

// No need for a separate throwBamlValidationError function in TypeScript
