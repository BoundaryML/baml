"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.CtxManager = void 0;
const native_1 = require("./native");
const async_hooks_1 = require("async_hooks");
class CtxManager {
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
    upsertTags(tags) {
        const manager = this.ctx.getStore();
        manager.upsertTags(tags);
    }
    get() {
        let store = this.ctx.getStore();
        if (store === undefined) {
            store = this.rt.createContextManager();
            this.ctx.enterWith(store);
        }
        return store;
    }
    startTraceSync(name, args) {
        const mng = this.get();
        // const clone = mng.deepClone()
        // this.ctx.enterWith(clone)
        return native_1.BamlSpan.new(this.rt, name, args, mng);
    }
    startTraceAsync(name, args) {
        const mng = this.get();
        const clone = mng.deepClone();
        this.ctx.enterWith(clone);
        return native_1.BamlSpan.new(this.rt, name, args, clone);
    }
    endTrace(span, response) {
        const manager = this.ctx.getStore();
        if (!manager) {
            console.error('Context lost before span could be finished\n');
            return;
        }
        span.finish(response, manager);
    }
    flush() {
        this.rt.flush();
    }
    onLogEvent(callback) {
        this.rt.setLogEventCallback(callback);
    }
    traceFnSync(name, func) {
        return ((...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [`arg${i}`]: arg, // generic way to label args
            }), {});
            const span = this.startTraceSync(name, params);
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
    }
    traceFnAync(name, func) {
        const funcName = name;
        return (async (...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [`arg${i}`]: arg, // generic way to label args
            }), {});
            const span = this.startTraceAsync(funcName, params);
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
    }
}
exports.CtxManager = CtxManager;
