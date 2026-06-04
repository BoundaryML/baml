/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
// define_function.ts — runtime factories for BAML callables, the Node analog
// of `define_function` in sdks/python/src/baml_core/__init__.py.
//
// Generated SDK code emits, per BAML function:
//   export const f = defineFunction("user.ns.f", "sync", ["a"]) as (a: A) => R;
//   export const f_async = defineFunction("user.ns.f", "async", ["a"]) as (a: A) => Promise<R>;
// and per instance method (inside the class body):
//   m = defineInstanceFunction("user.ns.C.m", "sync", ["self"]).bind(this) as () => R;
//
// The factory captures (fqn, mode, requiredNames, optionalNames) by closure;
// the returned callable zips positional args against requiredNames into kwargs,
// encodes it, calls the runtime, and decodes the result.
import { getRuntime } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET = Symbol('baml.UNSET');
function buildKwargs(args, requiredParamNames, optionalParamNames) {
    const positionalLimit = requiredParamNames.length;
    const hasOpts = optionalParamNames.length > 0;
    if (args.length > positionalLimit + (hasOpts ? 1 : 0)) {
        throw new TypeError(`got ${args.length} positional arguments but only ${positionalLimit} positional ` +
            `parameter names (${JSON.stringify(requiredParamNames)})`);
    }
    const built = {};
    for (let i = 0; i < args.length && i < positionalLimit; i++) {
        if (args[i] === UNSET)
            continue;
        built[requiredParamNames[i]] = args[i];
    }
    if (hasOpts && args.length > positionalLimit) {
        const opts = args[positionalLimit];
        if (opts === undefined || opts === UNSET) {
            return built;
        }
        if (opts === null || Array.isArray(opts) || typeof opts !== 'object') {
            throw new TypeError('optional arguments must be passed as an object');
        }
        const optionNames = new Set(optionalParamNames);
        for (const [key, value] of Object.entries(opts)) {
            if (!optionNames.has(key)) {
                throw new TypeError(`unknown optional argument ${JSON.stringify(key)}`);
            }
            if (value === undefined || value === UNSET)
                continue;
            built[key] = value;
        }
    }
    return built;
}
/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export function defineFunction(bamlFqn, mode, requiredParamNames, optionalParamNames) {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    if (mode === 'sync') {
        return (...args) => {
            const merged = buildKwargs(args, requiredNames, optionNames);
            const rt = getRuntime();
            const argsProto = encodeCallArgs(merged, /* syncMode */ true);
            const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args) => {
            const merged = buildKwargs(args, requiredNames, optionNames);
            const rt = getRuntime();
            const argsProto = encodeCallArgs(merged);
            const resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null, null);
            return decodeCallResult(resultBytes);
        };
    }
    throw new Error(`mode must be 'sync' or 'async', got ${JSON.stringify(mode)}`);
}
/**
 * Receiver-binding factory for instance methods. `paramNames[0]` is always
 * `"self"`. Codegen emits the binding as a class-field initializer
 * `m = defineInstanceFunction(...).bind(this) as () => R;`, so `.bind(self)`
 * captures the instance at construction time; the synthetic `self` param never
 * appears in the surface type.
 */
export function defineInstanceFunction(bamlFqn, mode, requiredParamNames, optionalParamNames) {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    const selfName = requiredNames[0] ?? 'self';
    const rest = requiredNames.slice(1);
    const makeKwargs = (self, args) => {
        const merged = buildKwargs(args, rest, optionNames);
        merged[selfName] = self;
        return merged;
    };
    return {
        bind(self) {
            if (mode === 'sync') {
                return (...args) => {
                    const merged = makeKwargs(self, args);
                    const rt = getRuntime();
                    const argsProto = encodeCallArgs(merged, /* syncMode */ true);
                    const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
                    return decodeCallResult(resultBytes);
                };
            }
            if (mode === 'async') {
                return async (...args) => {
                    const merged = makeKwargs(self, args);
                    const rt = getRuntime();
                    const argsProto = encodeCallArgs(merged);
                    const resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null, null);
                    return decodeCallResult(resultBytes);
                };
            }
            throw new Error(`mode must be 'sync' or 'async', got ${JSON.stringify(mode)}`);
        },
    };
}
//# sourceMappingURL=define_function.js.map