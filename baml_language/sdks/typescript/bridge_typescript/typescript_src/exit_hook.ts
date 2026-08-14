// Single-registration helper for runtime shutdown and event flushing.

import { flushEvents, shutdownRuntime } from './native.js';

let installed = false;

export function installFlushOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            await shutdownRuntime();
            flushEvents();
        } catch {
            /* ignore */
        }
    });
}
