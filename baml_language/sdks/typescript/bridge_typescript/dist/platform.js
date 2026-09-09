import { flushEvents } from './native.js';
export const supportsSyncStreamPulls = true;
export function handleExitPanic(code, _fallbackPanic) {
    flushEvents();
    process.exit(code);
}
//# sourceMappingURL=platform.js.map