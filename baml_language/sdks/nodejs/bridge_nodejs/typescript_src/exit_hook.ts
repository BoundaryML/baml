// exit_hook.ts — single-registration helper for flushEvents on process exit.
//
// Both index.ts and CtxManager constructor used to call process.once('exit',…)
// independently. This helper de-duplicates the registration so the hook is
// installed at most once per process regardless of how the module graph loads.

import { flushEvents } from './native';

let installed = false;

export function installFlushOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('exit', () => {
        try {
            flushEvents();
        } catch {
            // ignore
        }
    });
}
