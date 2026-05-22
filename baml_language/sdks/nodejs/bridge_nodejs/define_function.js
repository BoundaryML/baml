/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// define_function.ts — TS analog of bridge_python/baml_core/__init__.py:define_function.
//
// Creates a callable that:
//  1. Decodes positional + keyword args into a single kwargs object using
//     paramNames (the BAML callable's declared param order),
//  2. Encodes the kwargs via encodeCallArgs,
//  3. Invokes BamlRuntime.callFunctionSync or callFunction on the
//     process-global runtime (`getRuntime()`),
//  4. Decodes the result via decodeCallResult.
//
// The returned callable is intentionally typed as `(...args: unknown[]) => unknown`
// (sync) or `(...args: unknown[]) => Promise<unknown>` (async); the
// codegen-emitted call site asserts a precise typed signature at the
// definition.
Object.defineProperty(exports, "__esModule", { value: true });
exports.defineFunction = defineFunction;
const native_1 = require("./native");
const proto_1 = require("./proto");
const errors_1 = require("./errors");
function buildKwargs(paramNames, args) {
    // BAML calls are keyword-only per the spec. The codegen-emitted call
    // site can pass either:
    //   - A single object containing all kwargs (the canonical form), or
    //   - Positional args (one per param), zipped against paramNames.
    if (args.length === 1 && isPlainObject(args[0])) {
        return { ...args[0] };
    }
    const kwargs = {};
    for (let i = 0; i < args.length; i++) {
        const name = paramNames[i];
        if (name === undefined) {
            throw new TypeError(`Too many positional arguments for function: expected ${paramNames.length}, got ${args.length}`);
        }
        kwargs[name] = args[i];
    }
    return kwargs;
}
function isPlainObject(v) {
    if (v == null || typeof v !== 'object')
        return false;
    const proto = Object.getPrototypeOf(v);
    return proto === Object.prototype || proto === null;
}
function defineFunction(bamlFqn, mode, paramNames) {
    if (mode === 'sync') {
        return (...args) => {
            const kwargs = buildKwargs(paramNames, args);
            const argsProto = (0, proto_1.encodeCallArgs)(kwargs);
            try {
                const resultBytes = (0, native_1.getRuntime)().callFunctionSync(bamlFqn, argsProto, null, null, null);
                return (0, proto_1.decodeCallResult)(resultBytes);
            }
            catch (err) {
                throw (0, errors_1.wrapNativeError)(err);
            }
        };
    }
    return async (...args) => {
        const kwargs = buildKwargs(paramNames, args);
        const argsProto = (0, proto_1.encodeCallArgs)(kwargs);
        try {
            const resultBytes = await (0, native_1.getRuntime)().callFunction(bamlFqn, argsProto, null, null, null);
            return (0, proto_1.decodeCallResult)(resultBytes);
        }
        catch (err) {
            throw (0, errors_1.wrapNativeError)(err);
        }
    };
}
//# sourceMappingURL=define_function.js.map