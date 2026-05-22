/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// index.ts — mirrors bridge_python/python_src/baml_py/__init__.py
Object.defineProperty(exports, "__esModule", { value: true });
exports.Collector = exports.FunctionLog = exports.FunctionResult = exports.wrapNativeError = exports.BamlCancelledError = exports.BamlClientError = exports.BamlInvalidArgumentError = exports.BamlError = exports.CtxManager = exports.decodeCallResult = exports.encodeCallArgs = exports.Usage = exports.Timing = exports.flushEvents = exports.getVersion = exports.HostSpanManager = exports.BamlHandle = exports.AbortController = exports.BamlRuntime = void 0;
exports.callFunctionSync = callFunctionSync;
exports.callFunction = callFunction;
const native_1 = require("./native");
const proto_1 = require("./proto");
var native_2 = require("./native");
Object.defineProperty(exports, "BamlRuntime", { enumerable: true, get: function () { return native_2.BamlRuntime; } });
Object.defineProperty(exports, "AbortController", { enumerable: true, get: function () { return native_2.AbortController; } });
Object.defineProperty(exports, "BamlHandle", { enumerable: true, get: function () { return native_2.BamlHandle; } });
Object.defineProperty(exports, "HostSpanManager", { enumerable: true, get: function () { return native_2.HostSpanManager; } });
Object.defineProperty(exports, "getVersion", { enumerable: true, get: function () { return native_2.getVersion; } });
Object.defineProperty(exports, "flushEvents", { enumerable: true, get: function () { return native_2.flushEvents; } });
var native_3 = require("./native");
Object.defineProperty(exports, "Timing", { enumerable: true, get: function () { return native_3.Timing; } });
Object.defineProperty(exports, "Usage", { enumerable: true, get: function () { return native_3.Usage; } });
var proto_2 = require("./proto");
Object.defineProperty(exports, "encodeCallArgs", { enumerable: true, get: function () { return proto_2.encodeCallArgs; } });
Object.defineProperty(exports, "decodeCallResult", { enumerable: true, get: function () { return proto_2.decodeCallResult; } });
var ctx_manager_1 = require("./ctx_manager");
Object.defineProperty(exports, "CtxManager", { enumerable: true, get: function () { return ctx_manager_1.CtxManager; } });
const errors_1 = require("./errors");
var errors_2 = require("./errors");
Object.defineProperty(exports, "BamlError", { enumerable: true, get: function () { return errors_2.BamlError; } });
Object.defineProperty(exports, "BamlInvalidArgumentError", { enumerable: true, get: function () { return errors_2.BamlInvalidArgumentError; } });
Object.defineProperty(exports, "BamlClientError", { enumerable: true, get: function () { return errors_2.BamlClientError; } });
Object.defineProperty(exports, "BamlCancelledError", { enumerable: true, get: function () { return errors_2.BamlCancelledError; } });
Object.defineProperty(exports, "wrapNativeError", { enumerable: true, get: function () { return errors_2.wrapNativeError; } });
class FunctionResult {
    constructor(value) {
        this._value = value;
    }
    result() {
        return this._value;
    }
    toString() {
        return `FunctionResult(${JSON.stringify(this._value)})`;
    }
}
exports.FunctionResult = FunctionResult;
class FunctionLog {
    constructor(inner) { this._inner = inner; }
    get id() { return this._inner.id; }
    get functionName() { return this._inner.functionName; }
    get timing() { return this._inner.timing; }
    get usage() { return this._inner.usage; }
    get calls() { return this._inner.calls; }
    get tags() { return this._inner.tags; }
    // FIXME: Returns null for both "no serialized result" (bytes == null) and a legitimate
    // BAML null result (decodeCallResult returns null). Legacy engine/ had no result getter
    // on FunctionLog at all. bridge_python has the same ambiguity (None for both cases).
    // Leaving as-is for parity with bridge_python; narrow edge case in practice.
    get result() {
        const bytes = this._inner.result;
        if (bytes == null)
            return null;
        return (0, proto_1.decodeCallResult)(bytes);
    }
}
exports.FunctionLog = FunctionLog;
class Collector {
    constructor(name) { this._inner = new native_1.Collector(name ?? null); }
    get name() { return this._inner.name; }
    get logs() {
        return this._inner.logs.map((l) => new FunctionLog(l));
    }
    get last() {
        const l = this._inner.last;
        return l ? new FunctionLog(l) : null;
    }
    get usage() { return this._inner.usage; }
    clear() { return this._inner.clear(); }
    id(functionLogId) {
        const l = this._inner.id(functionLogId);
        return l ? new FunctionLog(l) : null;
    }
    /** Internal: get native collector for passing to Rust */
    _native() { return this._inner; }
}
exports.Collector = Collector;
function callFunctionSync(rt, functionName, kwargs, ctx, collectors, abortController) {
    const argsProto = (0, proto_1.encodeCallArgs)(kwargs);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    try {
        const resultBytes = rt.callFunctionSync(functionName, argsProto, ctx ?? null, nativeCollectors, abortController ?? null);
        return new FunctionResult((0, proto_1.decodeCallResult)(resultBytes));
    }
    catch (err) {
        throw (0, errors_1.wrapNativeError)(err);
    }
}
async function callFunction(rt, functionName, kwargs, ctx, collectors, abortController) {
    const argsProto = (0, proto_1.encodeCallArgs)(kwargs);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    try {
        const resultBytes = await rt.callFunction(functionName, argsProto, ctx ?? null, nativeCollectors, abortController ?? null);
        return new FunctionResult((0, proto_1.decodeCallResult)(resultBytes));
    }
    catch (err) {
        throw (0, errors_1.wrapNativeError)(err);
    }
}
// Register flush on process exit (once to prevent duplicate handlers on module reload)
process.once('exit', () => {
    try {
        (0, native_1.flushEvents)();
    }
    catch (_) { /* ignore */ }
});
//# sourceMappingURL=index.js.map