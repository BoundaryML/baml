// Single-registration helper for runtime shutdown and event flushing.

import { flushEvents, shutdownRuntime, _waitForRuntimeIdle } from './native.js';

let installed = false;

export function installFlushOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            // A registered host callable is ownership, not activity, and no
            // longer keeps the event loop alive on its own. This pending
            // promise does: it holds Node open until already-started BAML
            // work — including a background `spawn` that calls back into JS,
            // which may itself re-enter BAML — has settled, without closing
            // admission for that re-entry.
            await _waitForRuntimeIdle();
            await shutdownRuntime();
            flushEvents();
        } catch {
            /* ignore */
        }
    });
}
