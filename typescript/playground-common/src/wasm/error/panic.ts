import { WasmPanicRegistry } from './WasmPanicRegistry'

/**
 * Branded type for Wasm panics.
 */
export type WasmPanic = Error & { name: 'RuntimeError' }

/**
 * Returns true if the given error is a Wasm panic.
 */
export function isWasmPanic(error: Error): error is WasmPanic {
  return error.name === 'RuntimeError'
}

/**
 * Set up a global registry for Wasm panics.
 * This allows us to retrieve the panic message from the Wasm panic hook,
 * which is not possible otherwise.
 */
let globalWithWasm = globalThis as typeof global & {
  PRISMA_WASM_PANIC_REGISTRY: WasmPanicRegistry
}

// Only do this once.
globalWithWasm.PRISMA_WASM_PANIC_REGISTRY = new WasmPanicRegistry()

export function getWasmError(error: WasmPanic) {
  const message: string = globalWithWasm.PRISMA_WASM_PANIC_REGISTRY.get()
  const stack = [message, ...(error.stack || 'NO_BACKTRACE').split('\n').slice(1)].join('\n')

  return { message, stack }
}
