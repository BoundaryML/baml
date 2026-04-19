import { WasmPanicRegistry } from './WasmPanicRegistry';

export type WasmPanic = Error & { name: 'RuntimeError' };

export function isWasmPanic(error: unknown): error is WasmPanic {
  return error instanceof Error && error.name === 'RuntimeError';
}

const globalWithWasm = globalThis as typeof globalThis & {
  BAML_WASM_PANIC_REGISTRY: WasmPanicRegistry;
};

// Initialize once
if (!globalWithWasm.BAML_WASM_PANIC_REGISTRY) {
  globalWithWasm.BAML_WASM_PANIC_REGISTRY = new WasmPanicRegistry();
}

export function getWasmPanicRegistry(): WasmPanicRegistry {
  return globalWithWasm.BAML_WASM_PANIC_REGISTRY;
}

export function getWasmError(error: WasmPanic): { message: string; stack: string } {
  const registry = getWasmPanicRegistry();
  const panicMessage = registry.get();
  const message = panicMessage || error.message || 'Unknown WASM panic';
  const stack = [
    message,
    ...(error.stack || 'NO_BACKTRACE').split('\n').slice(1),
  ].join('\n');

  return { message, stack };
}

export function handleWasmError(
  e: Error,
  cmd: string,
  onError?: (errorMessage: string) => void,
): { message: string; isPanic: boolean; stack: string | undefined } {
  const getErrorMessage = () => {
    if (isWasmPanic(e)) {
      const { message, stack } = getWasmError(e);
      const msg = `WASM errored when invoking ${cmd}. It resulted in a Wasm panic.\n${message}`;
      return { message: msg, isPanic: true, stack };
    }

    const msg = `WASM errored when invoking ${cmd}.\n${e.message}`;
    return { message: msg, isPanic: false, stack: e.stack };
  };

  const { message, isPanic, stack } = getErrorMessage();

  if (isPanic) {
    console.warn(`WASM errored (panic) with: ${message}\n\n${stack}`);
  } else {
    console.warn(`WASM errored with: ${message}\n\n${stack}`);
  }

  if (onError) {
    onError(`WASM errored with: ${message}`);
  }

  return { message, isPanic, stack };
}
