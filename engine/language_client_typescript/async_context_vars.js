"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlCtxManager = void 0;
const native_1 = require("./native");
const async_hooks_1 = require("async_hooks");
class BamlCtxManager {
    rt;
    ctx;
    constructor(rt) {
        this.rt = rt;
        this.ctx = new async_hooks_1.AsyncLocalStorage();
        this.ctx.enterWith(rt.createContextManager());
        process.on('exit', () => {
            this.rt.flush();
        });
    }
    allowResets() {
        let store = this.ctx.getStore();
        if (store === undefined) {
            return true;
        }
        if (store.contextDepth() > 0) {
            return false;
        }
        return true;
    }
    reset() {
        this.ctx = new async_hooks_1.AsyncLocalStorage();
        this.ctx.enterWith(this.rt.createContextManager());
    }
    upsertTags(tags) {
        const manager = this.ctx.getStore();
        manager.upsertTags(tags);
    }
    cloneContext() {
        let store = this.ctx.getStore();
        if (store === undefined) {
            store = this.rt.createContextManager();
            this.ctx.enterWith(store);
        }
        return store.deepClone();
    }
    startTrace(name, args) {
        const mng = this.cloneContext();
        return [mng, native_1.BamlSpan.new(this.rt, name, args, mng)];
    }
    endTrace(span, response) {
        const manager = this.ctx.getStore();
        if (!manager) {
            console.error('Context lost before span could be finished\n');
            return;
        }
        try {
            span.finish(response, manager);
        }
        catch (e) {
            console.error('BAML internal error', e);
        }
    }
    flush() {
        this.rt.flush();
    }
    onLogEvent(callback) {
        if (!callback) {
            this.rt.setLogEventCallback(undefined);
            return;
        }
        this.rt.setLogEventCallback((error, param) => {
            if (!error) {
                callback(param);
            }
        });
    }
    traceFnSync(name, func) {
        return ((...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [`arg${i}`]: arg, // generic way to label args
            }), {});
            const [mng, span] = this.startTrace(name, params);
            this.ctx.run(mng, () => {
                try {
                    const response = func(...args);
                    this.endTrace(span, response);
                    return response;
                }
                catch (e) {
                    this.endTrace(span, e);
                    throw e;
                }
            });
        });
    }
    traceFnAsync(name, func) {
        const funcName = name;
        return (async (...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [`arg${i}`]: arg, // generic way to label args
            }), {});
            const [mng, span] = this.startTrace(name, params);
            await this.ctx.run(mng, async () => {
                try {
                    const response = await func(...args);
                    this.endTrace(span, response);
                    return response;
                }
                catch (e) {
                    this.endTrace(span, e);
                    throw e;
                }
            });
        });
    }
}
exports.BamlCtxManager = BamlCtxManager;
