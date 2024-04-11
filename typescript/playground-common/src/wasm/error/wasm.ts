import { WasmPanicRegistry } from './WasmPanicRegistry'

/**
 * Set up a global registry for Wasm panics.
 * This allows us to retrieve the panic message from the Wasm panic hook,
 * which is not possible otherwise.
 */

const globalWithWasm = globalThis as typeof global & {
  PRISMA_WASM_PANIC_REGISTRY: WasmPanicRegistry
}

globalWithWasm.PRISMA_WASM_PANIC_REGISTRY = new WasmPanicRegistry()
