// exit_hook.ts — single-registration helper for flushEvents on process exit.
// Both index.ts and the CtxManager constructor used to call
// process.once('exit', …) independently; consolidate to one registration so
// `process.listenerCount('exit')` increases by exactly one per process.

import { flushEvents } from './native';

let installed = false;

export function installFlushOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('exit', () => {
        try {
            flushEvents();
        } catch {
            /* ignore */
        }
    });
}
