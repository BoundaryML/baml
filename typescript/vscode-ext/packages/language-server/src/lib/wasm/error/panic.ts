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

export function getWasmError(error: WasmPanic) {
  const globalWithWasm = globalThis as typeof global & {
    PRISMA_WASM_PANIC_REGISTRY: WasmPanicRegistry
  }

  const message: string = globalWithWasm.PRISMA_WASM_PANIC_REGISTRY.get()
  const stack = [message, ...(error.stack || 'NO_BACKTRACE').split('\n').slice(1)].join('\n')

  return { message, stack }
}
