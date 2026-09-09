// Single-registration helper for runtime shutdown and event flushing.
import { flushEvents, shutdownRuntime, _waitForRuntimeIdle } from './native.js';
let installed = false;
export function installFlushOnExit() {
    if (installed)
        return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            // Registration ownership is not activity. Keep admission open
            // until already-started work (including callback re-entry) settles.
            await _waitForRuntimeIdle();
            await shutdownRuntime();
            flushEvents();
        }
        catch {
            /* ignore */
        }
    });
}
//# sourceMappingURL=exit_hook.js.map