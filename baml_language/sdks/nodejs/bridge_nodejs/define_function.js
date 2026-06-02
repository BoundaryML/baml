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
// The factory captures (fqn, mode, paramNames) by closure; the returned
// callable zips positional args against paramNames into a kwargs object,
// encodes it, calls the runtime, and decodes the result.
import { getRuntime } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET = Symbol('baml.UNSET');
function buildKwargs(args, paramNames, requiredPositionalCount) {
    const positionalLimit = requiredPositionalCount ?? paramNames.length;
    if (args.length > positionalLimit) {
        throw new TypeError(`got ${args.length} positional arguments but only ${positionalLimit} positional ` +
            `parameter names (${JSON.stringify(paramNames.slice(0, positionalLimit))})`);
    }
    const built = {};
    for (let i = 0; i < args.length && i < paramNames.length; i++) {
        if (args[i] === UNSET)
            continue;
        built[paramNames[i]] = args[i];
    }
    return built;
}
/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export function defineFunction(bamlFqn, mode, paramNames, requiredPositionalCount) {
    const names = [...paramNames];
    if (mode === 'sync') {
        return (...args) => {
            const merged = buildKwargs(args, names, requiredPositionalCount);
            const rt = getRuntime();
            const argsProto = encodeCallArgs(merged, /* syncMode */ true);
            const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args) => {
            const merged = buildKwargs(args, names, requiredPositionalCount);
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
export function defineInstanceFunction(bamlFqn, mode, paramNames) {
    const names = [...paramNames];
    const selfName = names[0] ?? 'self';
    const rest = names.slice(1);
    const makeKwargs = (self, args) => {
        const merged = buildKwargs(args, rest);
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