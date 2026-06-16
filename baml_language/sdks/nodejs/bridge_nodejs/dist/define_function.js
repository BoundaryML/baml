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
import { getRuntime, newFunctionCall as nativeNewFunctionCall, } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET = Symbol('baml.UNSET');
function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
function attachCallContext(ctx, callId) {
    ctx?._attachCallId(callId.toString());
    return {
        detach() {
            ctx?._detachCallId(callId.toString());
        },
    };
}
function buildArgs(args, requiredParamNames, optionalParamNames) {
    const positionalLimit = requiredParamNames.length;
    if (args.length > positionalLimit + 1) {
        throw new TypeError(`got ${args.length} positional arguments but only ${positionalLimit} positional ` +
            `parameter names (${JSON.stringify(requiredParamNames)})`);
    }
    const built = {};
    for (let i = 0; i < args.length && i < positionalLimit; i++) {
        if (args[i] === UNSET)
            continue;
        built[requiredParamNames[i]] = args[i];
    }
    let ctx;
    if (args.length > positionalLimit) {
        const opts = args[positionalLimit];
        if (opts === undefined || opts === UNSET) {
            return { kwargs: built };
        }
        if (opts === null || Array.isArray(opts) || typeof opts !== 'object') {
            throw new TypeError('optional arguments must be passed as an object');
        }
        const optionNames = new Set(optionalParamNames);
        for (const [key, value] of Object.entries(opts)) {
            if (key === '$ctx') {
                if (value !== undefined && value !== UNSET) {
                    ctx = value;
                }
                continue;
            }
            if (!optionNames.has(key)) {
                throw new TypeError(`unknown optional argument ${JSON.stringify(key)}`);
            }
            if (value === undefined || value === UNSET)
                continue;
            built[key] = value;
        }
    }
    return { kwargs: built, ctx };
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
            const { kwargs: merged, ctx } = buildArgs(args, requiredNames, optionNames);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(merged, { syncMode: true, callId });
            const callCtxBinding = attachCallContext(ctx, callId);
            let resultBytes;
            try {
                resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null);
            }
            finally {
                callCtxBinding.detach();
            }
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args) => {
            const { kwargs: merged, ctx } = buildArgs(args, requiredNames, optionNames);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(merged, { callId });
            const callCtxBinding = attachCallContext(ctx, callId);
            let resultBytes;
            try {
                resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null);
            }
            finally {
                callCtxBinding.detach();
            }
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
    const makeArgs = (self, args) => {
        const built = buildArgs(args, rest, optionNames);
        built.kwargs[selfName] = self;
        return built;
    };
    return {
        bind(self) {
            if (mode === 'sync') {
                return (...args) => {
                    const { kwargs: merged, ctx } = makeArgs(self, args);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(merged, { syncMode: true, callId });
                    const callCtxBinding = attachCallContext(ctx, callId);
                    let resultBytes;
                    try {
                        resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null);
                    }
                    finally {
                        callCtxBinding.detach();
                    }
                    return decodeCallResult(resultBytes);
                };
            }
            if (mode === 'async') {
                return async (...args) => {
                    const { kwargs: merged, ctx } = makeArgs(self, args);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(merged, { callId });
                    const callCtxBinding = attachCallContext(ctx, callId);
                    let resultBytes;
                    try {
                        resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null);
                    }
                    finally {
                        callCtxBinding.detach();
                    }
                    return decodeCallResult(resultBytes);
                };
            }
            throw new Error(`mode must be 'sync' or 'async', got ${JSON.stringify(mode)}`);
        },
    };
}
//# sourceMappingURL=define_function.js.map