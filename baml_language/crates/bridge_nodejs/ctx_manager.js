/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// ctx_manager.ts — mirrors bridge_python/python_src/baml_py/ctx_manager.py
// Uses AsyncLocalStorage for async context isolation (Node.js built-in).
Object.defineProperty(exports, "__esModule", { value: true });
exports.CtxManager = void 0;
const node_async_hooks_1 = require("node:async_hooks");
const native_1 = require("./native");
let exitHookInstalled = false;
class CtxManager {
    constructor(rt) {
        this.rt = rt;
        this.ctx = new node_async_hooks_1.AsyncLocalStorage();
        // FIXME: Eagerly creates HostSpanManager before the runtime may be fully initialized.
        // Legacy engine/ was also eager (rt.createContextManager() in constructor).
        // bridge_python lazily creates HostSpanManager per-thread on first access.
        // Leaving as-is: get() and reset() already create fresh managers, and the legacy
        // engine/ had the same eager pattern without reported issues.
        this.ctx.enterWith(new native_1.HostSpanManager());
        if (!exitHookInstalled) {
            exitHookInstalled = true;
            process.once('exit', () => {
                try {
                    (0, native_1.flushEvents)();
                }
                catch { }
            });
        }
    }
    get() {
        let mgr = this.ctx.getStore();
        if (!mgr) {
            mgr = new native_1.HostSpanManager();
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
        this.ctx.enterWith(new native_1.HostSpanManager());
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
        (0, native_1.flushEvents)();
    }
}
exports.CtxManager = CtxManager;
//# sourceMappingURL=ctx_manager.js.map