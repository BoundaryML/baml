/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// ctx_manager.ts — mirrors bridge_python/python_src/baml_py/ctx_manager.py
// Uses AsyncLocalStorage for async context isolation (Node.js built-in).
import { AsyncLocalStorage } from 'node:async_hooks';
import { HostSpanManager, flushEvents } from './native.js';
import { installFlushOnExit } from './exit_hook.js';
export class CtxManager {
    rt;
    ctx;
    constructor(rt) {
        this.rt = rt;
        this.ctx = new AsyncLocalStorage();
        // FIXME: Eagerly creates HostSpanManager before the runtime may be fully initialized.
        // Legacy engine/ was also eager (rt.createContextManager() in constructor).
        // bridge_python lazily creates HostSpanManager per-thread on first access.
        // Leaving as-is: get() and reset() already create fresh managers, and the legacy
        // engine/ had the same eager pattern without reported issues.
        this.ctx.enterWith(new HostSpanManager());
        installFlushOnExit();
    }
    get() {
        let mgr = this.ctx.getStore();
        if (!mgr) {
            mgr = new HostSpanManager();
            this.ctx.enterWith(mgr);
        }
        return mgr;
    }
    allowReset() {
        const mgr = this.ctx.getStore();
        if (!mgr)
            return true;
        return mgr.contextDepth() === 0;
    }
    reset() {
        this.ctx.enterWith(new HostSpanManager());
    }
    cloneContext() {
        const mgr = this.get();
        return mgr.deepClone();
    }
    upsertTags(tags) {
        this.get().upsertTags(tags);
    }
    traceFn(name, fn) {
        const self = this;
        return function (...args) {
            const mgr = self.cloneContext();
            const argsObj = {};
            args.forEach((arg, i) => { argsObj[`arg${i}`] = arg; });
            mgr.enter(name, argsObj);
            try {
                const result = self.ctx.run(mgr, () => fn.apply(this, args));
                mgr.exitOk();
                return result;
            }
            catch (e) {
                mgr.exitError(String(e));
                throw e;
            }
        };
    }
    traceFnAsync(name, fn) {
        const self = this;
        return async function (...args) {
            const mgr = self.cloneContext();
            const argsObj = {};
            args.forEach((arg, i) => { argsObj[`arg${i}`] = arg; });
            mgr.enter(name, argsObj);
            try {
                const result = await self.ctx.run(mgr, () => fn.apply(this, args));
                mgr.exitOk();
                return result;
            }
            catch (e) {
                mgr.exitError(String(e));
                throw e;
            }
        };
    }
    flush() {
        flushEvents();
    }
}
//# sourceMappingURL=ctx_manager.js.map