// Single-registration helper for runtime shutdown.

import { shutdownRuntime } from './native.js';

let installed = false;

export function installShutdownOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            await shutdownRuntime();
        } catch {
            /* ignore */
        }
    });
}
