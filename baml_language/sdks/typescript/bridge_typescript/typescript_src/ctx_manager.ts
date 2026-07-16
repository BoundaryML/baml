// ctx_manager.ts — mirrors bridge_python/python_src/baml_py/ctx_manager.py
// Uses AsyncLocalStorage for async context isolation (Node.js built-in).

import { AsyncLocalStorage } from 'node:async_hooks';
import { HostSpanManager, flushEvents } from './native.js';
import { installFlushOnExit } from './exit_hook.js';

export class CtxManager {
    private rt: unknown;
    private ctx: AsyncLocalStorage<HostSpanManager>;

    constructor(rt: unknown) {
        this.rt = rt;
        this.ctx = new AsyncLocalStorage<HostSpanManager>();
        // FIXME: Eagerly creates HostSpanManager before the runtime may be fully initialized.
        // Legacy engine/ was also eager (rt.createContextManager() in constructor).
        // bridge_python lazily creates HostSpanManager per-thread on first access.
        // Leaving as-is: get() and reset() already create fresh managers, and the legacy
        // engine/ had the same eager pattern without reported issues.
        this.ctx.enterWith(new HostSpanManager());

        installFlushOnExit();
    }

    get(): HostSpanManager {
        let mgr = this.ctx.getStore();
        if (!mgr) {
            mgr = new HostSpanManager();
            this.ctx.enterWith(mgr);
        }
        return mgr;
    }

    allowReset(): boolean {
        const mgr = this.ctx.getStore();
        if (!mgr) return true;
        return mgr.contextDepth() === 0;
    }

    reset(): void {
        this.ctx.enterWith(new HostSpanManager());
    }

    cloneContext(): HostSpanManager {
        const mgr = this.get();
        return mgr.deepClone();
    }

    upsertTags(tags: Record<string, string>): void {
        this.get().upsertTags(tags);
    }

    traceFn<T>(name: string, fn: (...args: unknown[]) => T): (...args: unknown[]) => T {
        const self = this;
        return function (this: unknown, ...args: unknown[]): T {
            const mgr = self.cloneContext();
            const argsObj: Record<string, unknown> = {};
            args.forEach((arg, i) => { argsObj[`arg${i}`] = arg; });
            mgr.enter(name, argsObj);
            try {
                const result = self.ctx.run(mgr, () => fn.apply(this, args));
                mgr.exitOk();
                return result;
            } catch (e) {
                mgr.exitError(String(e));
                throw e;
            }
        };
    }

    traceFnAsync<T>(name: string, fn: (...args: unknown[]) => Promise<T>): (...args: unknown[]) => Promise<T> {
        const self = this;
        return async function (this: unknown, ...args: unknown[]): Promise<T> {
            const mgr = self.cloneContext();
            const argsObj: Record<string, unknown> = {};
            args.forEach((arg, i) => { argsObj[`arg${i}`] = arg; });
            mgr.enter(name, argsObj);
            try {
                const result = await self.ctx.run(mgr, () => fn.apply(this, args));
                mgr.exitOk();
                return result;
            } catch (e) {
                mgr.exitError(String(e));
                throw e;
            }
        };
    }

    flush(): void {
        flushEvents();
    }
}
