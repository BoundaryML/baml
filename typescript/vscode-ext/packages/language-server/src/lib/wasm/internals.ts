import { getWasmError, isWasmPanic, WasmPanic } from './error/panic'

const packageJson = require('../../../../package.json') // eslint-disable-line

/* eslint-disable @typescript-eslint/no-unsafe-member-access,@typescript-eslint/no-unsafe-return */

/**
 * Lookup version
 */
export function getVersion(): string {
  if (!packageJson || !packageJson.Baml || !packageJson.Baml.enginesVersion) {
    return 'latest'
  }
  return packageJson.Baml.enginesVersion
}

/**
 * Gets Engines Version from package.json, dependencies, `@Baml/language-wasm`
 * @returns Something like `2.26.0-23.9b816b3aa13cc270074f172f30d6eda8a8ce867d`
 */
export function getEnginesVersion(): string {
  return packageJson.dependencies['@Baml/language-wasm']
}

/**
 * Gets CLI Version from package.json, Baml, cliVersion
 * @returns Something like `2.27.0-dev.50`
 */
export function getCliVersion(): string {
  return packageJson.Baml.cliVersion
}

export function handleWasmError(e: Error, cmd: string, onError?: (errorMessage: string) => void) {
  const getErrorMessage = () => {
    if (isWasmPanic(e)) {
      const { message, stack } = getWasmError(e)
      const msg = `language-wasm errored when invoking ${cmd}. It resulted in a Wasm panic.\n${message}`
      return { message: msg, isPanic: true, stack }
    }

    const msg = `language-wasm errored when invoking ${cmd}.\n${e.message}`
    return { message: msg, isPanic: false, stack: e.stack }
  }

  const { message, isPanic, stack } = getErrorMessage()

  if (isPanic) {
    console.warn(`language-wasm errored (panic) with: ${message}\n\n${stack}`)
  } else {
    console.warn(`language-wasm errored with: ${message}\n\n${stack}`)
  }

  if (onError) {
    onError(
      // Note: VS Code strips newline characters from the message
      `language-wasm errored with: -- ${message} -- For the full output check the "Baml Language Server" output. In the menu, click "View", then Output and select "Baml Language Server" in the drop-down menu.`,
    )
  }
}

export function handleFormatPanic(tryCb: () => void) {
  try {
    return tryCb()
  } catch (e: unknown) {
    throw getWasmError(e as WasmPanic)
  }
}
