import { getWasmPanicRegistry } from './panic';

let installed = false;
let panicCallback: ((message: string) => void) | null = null;

/**
 * Register a callback to be called when a WASM panic is detected.
 * The callback receives the panic message.
 */
export function onWasmPanic(callback: (message: string) => void): void {
  panicCallback = callback;
}

/**
 * Intercepts console.error to capture WASM panic messages.
 * Call this once before initializing WASM.
 *
 * When Rust panics with console_error_panic_hook, it logs to console.error.
 * This interceptor captures that message and calls the registered callback.
 */
export function installWasmPanicHook(): void {
  if (installed) return;
  installed = true;

  const originalConsoleError = console.error;
  const registry = getWasmPanicRegistry();

  console.error = (...args: unknown[]) => {
    // Check if this looks like a Rust panic message
    const firstArg = args[0];
    if (typeof firstArg === 'string') {
      // Rust panics via console_error_panic_hook typically start with "panicked at"
      if (
        firstArg.includes('panicked at') ||
        firstArg.includes('wasm-bindgen')
      ) {
        const message = args.map((a) => String(a)).join(' ');
        registry.set_message(message);
        // Notify immediately when panic is detected
        if (panicCallback) {
          panicCallback(message);
        }
      }
    }

    // Always call the original
    originalConsoleError.apply(console, args);
  };
}
