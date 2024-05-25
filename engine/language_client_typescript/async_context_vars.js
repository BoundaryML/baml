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
        return this.ctx.getStore();
    }
    startTraceSync(name, args) {
        const mng = this.get();
        return native_1.BamlSpanPy.new(this.rt, name, args, mng);
    }
    startTraceAsync(name, args) {
        const mng = this.get();
        const clone = mng.deepClone();
        this.ctx.enterWith(clone);
        return native_1.BamlSpanPy.new(this.rt, name, args, clone);
    }
    async endTrace(span, response) {
        await span.finish(response, this.get());
    }
    traceFnSync(name, func) {
        return ((...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [func.length > i ? func.arguments[i].name : `<arg:${i}>`]: arg,
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
    traceFnAync(func) {
        const funcName = func.name;
        return (async (...args) => {
            const params = args.reduce((acc, arg, i) => ({
                ...acc,
                [func.length > i ? func.arguments[i].name : `<arg:${i}>`]: arg,
            }), {});
            const span = this.startTraceAsync(funcName, params);
            try {
                const response = await func(...args);
                await this.endTrace(span, response);
                return response;
            }
            catch (e) {
                await this.endTrace(span, e);
                throw e;
            }
        });
    }
}
exports.CtxManager = CtxManager;
