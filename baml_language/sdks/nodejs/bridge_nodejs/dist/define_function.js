/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
// define_function.ts — runtime factories for BAML callables, the Node analog
// of `define_function` in sdks/python/src/baml_bridge/__init__.py.
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
import { cancelFunctionCall as nativeCancelFunctionCall, getRuntime as nativeGetRuntime, newFunctionCall as nativeNewFunctionCall, } from './native.js';
import { wrapNativeError } from './errors.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
import { lowerTypeToWireTy } from './wire_ty.js';
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET = Symbol('baml.UNSET');
/**
 * Resolve the caller's `$types` option onto the callee's own generic params, in
 * declaration order. Mirrors Python's `_resolve_types_kwarg`: `$types` is an
 * optional object keyed by param name. Missing bindings are left undefined so
 * the engine can infer them from the argument values; unknown names still fail
 * at the host boundary.
 */
function resolveTypesOption(typesOpt, typeParams) {
    if (typeParams.length === 0) {
        if (typesOpt !== undefined && typesOpt !== null) {
            throw new TypeError('$types is not accepted here: this function/method declares no generic type ' +
                'parameters of its own');
        }
        return [];
    }
    const example = `{ ${JSON.stringify(typeParams[0])}: 'int' }`;
    if (typesOpt === undefined || typesOpt === null) {
        return typeParams.map(() => undefined);
    }
    if (typeof typesOpt !== 'object' || Array.isArray(typesOpt)) {
        throw new TypeError(`$types must be an object mapping type-parameter names to types (e.g. ` +
            `$types: ${example}); got ${typeof typesOpt}`);
    }
    const obj = typesOpt;
    const extra = Object.keys(obj).filter((k) => !typeParams.includes(k));
    if (extra.length) {
        throw new TypeError(`$types has unknown type parameter(s) ${JSON.stringify(extra)}; expected exactly ` +
            `${JSON.stringify(typeParams)}.`);
    }
    return typeParams.map((n) => obj[n]);
}
/** The `$types` field of a generic receiver instance (its class TypeVar
 * bindings), or `undefined` when the receiver carries none. The TS analog of
 * Python's `pydantic_instance_type_args(self)`. */
function receiverTypes(self) {
    if (self != null && typeof self === 'object') {
        const t = self.$types;
        if (t != null && typeof t === 'object') {
            return t;
        }
    }
    return undefined;
}
/**
 * Build the named, order-preserving wire `type_args` for a generic call. Mirrors
 * Python's `_build_type_args`: enclosing class params (recovered from the `self`
 * receiver's `$types`) come first, then the callee's own params (`$types`
 * option) — De Bruijn order. Class params are seeded only when the receiver
 * actually carries them, so a non-generic receiver keeps the engine's
 * recover-from-receiver behavior. Returns `[]` when the call binds nothing.
 */
function buildTypeArgs(self, typesOpt, typeParams, classTypeParams) {
    const wire = [];
    const classTypes = classTypeParams.length ? receiverTypes(self) : undefined;
    if (classTypes) {
        for (const name of classTypeParams) {
            wire.push([name, lowerTypeToWireTy(classTypes[name])]);
        }
    }
    const resolved = resolveTypesOption(typesOpt, typeParams);
    const bound = typeParams.flatMap((name, i) => {
        const ty = resolved[i];
        return ty === undefined || ty === null ? [] : [[name, ty]];
    });
    if (bound.length > 0) {
        if (classTypeParams.length && !classTypes) {
            // The method's own params sit after the class prefix in De Bruijn
            // order; without recovered class args we can't position them.
            throw new TypeError('$types on a generic method requires a generic receiver carrying its class type ' +
                'args (a `$types` field on the instance)');
        }
        bound.forEach(([name, ty]) => {
            wire.push([name, lowerTypeToWireTy(ty)]);
        });
    }
    return wire;
}
function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
function getRuntime() {
    try {
        return nativeGetRuntime();
    }
    catch (error) {
        throw wrapNativeError(error);
    }
}
function attachCallContext(suppliedCtx, signal, callId) {
    const callIdString = callId.toString();
    // `$ctx` is a reusable, potentially shared cancellation scope. A per-call
    // AbortSignal must not abort that whole scope: cancel only this call id.
    const abort = () => nativeCancelFunctionCall(callIdString);
    let listening = false;
    suppliedCtx?._attachCallId(callIdString);
    try {
        if (signal) {
            signal.addEventListener('abort', abort, { once: true });
            listening = true;
            // Close the race between reading `aborted` and installing the listener.
            if (signal.aborted)
                abort();
        }
    }
    catch (error) {
        if (signal && listening)
            signal.removeEventListener('abort', abort);
        suppliedCtx?._detachCallId(callIdString);
        throw error;
    }
    return {
        detach() {
            if (signal && listening)
                signal.removeEventListener('abort', abort);
            suppliedCtx?._detachCallId(callIdString);
        },
    };
}
function buildArgs(args, requiredParamNames) {
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
    let signal;
    let types;
    if (args.length > positionalLimit) {
        const opts = args[positionalLimit];
        if (opts === undefined || opts === UNSET) {
            return { kwargs: built };
        }
        if (opts === null || Array.isArray(opts) || typeof opts !== 'object') {
            throw new TypeError('optional arguments must be passed as an object');
        }
        for (const [key, value] of Object.entries(opts)) {
            if (key === '$ctx') {
                if (value !== undefined && value !== UNSET) {
                    ctx = value;
                }
                continue;
            }
            if (key === '$signal') {
                if (value !== undefined && value !== UNSET) {
                    if (value === null
                        || typeof value !== 'object'
                        || typeof value.aborted !== 'boolean'
                        || typeof value.addEventListener !== 'function'
                        || typeof value.removeEventListener !== 'function') {
                        throw new TypeError('$signal must be an AbortSignal');
                    }
                    signal = value;
                }
                continue;
            }
            if (key === '$types') {
                // Generic-call TypeVar bindings — captured here and lowered to
                // wire `type_args` by the factory, never sent as a kwarg value.
                if (value !== undefined && value !== UNSET) {
                    types = value;
                }
                continue;
            }
            if (value === undefined || value === UNSET)
                continue;
            // Preserve unknown host keyword names on the wire. The shared
            // bridge/engine validation then returns the same structured
            // baml.errors.InvalidArgument value as Python.
            built[key] = value;
        }
    }
    return { kwargs: built, ctx, signal, types };
}
/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export function defineFunction(bamlFqn, mode, requiredParamNames, _optionalParamNames, generics) {
    const requiredNames = [...requiredParamNames];
    // A free function / static method binds only its OWN generic params (a
    // generic receiver is never in play here), so `classTypeParams` is unused.
    const typeParams = generics?.typeParams ?? [];
    const typeArgsFor = (built) => buildTypeArgs(undefined, built.types, typeParams, []);
    if (mode === 'sync') {
        return (...args) => {
            const built = buildArgs(args, requiredNames);
            const typeArgs = typeArgsFor(built);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(built.kwargs, { syncMode: true, callId, typeArgs });
            const callCtxBinding = attachCallContext(built.ctx, built.signal, callId);
            let resultBytes;
            try {
                resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null);
            }
            catch (error) {
                throw wrapNativeError(error);
            }
            finally {
                callCtxBinding.detach();
            }
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args) => {
            const built = buildArgs(args, requiredNames);
            const typeArgs = typeArgsFor(built);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(built.kwargs, { callId, typeArgs });
            const callCtxBinding = attachCallContext(built.ctx, built.signal, callId);
            let resultBytes;
            try {
                resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null);
            }
            catch (error) {
                throw wrapNativeError(error);
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
export function defineInstanceFunction(bamlFqn, mode, requiredParamNames, _optionalParamNames, generics) {
    const requiredNames = [...requiredParamNames];
    const selfName = requiredNames[0] ?? 'self';
    const rest = requiredNames.slice(1);
    // An instance method binds its own `<...>` params (caller's `$types`) AND
    // the enclosing class's params, recovered from the `self` receiver's
    // `$types` field. Mirrors Python's `class_type_params` for instance methods.
    const typeParams = generics?.typeParams ?? [];
    const classTypeParams = generics?.classTypeParams ?? [];
    const makeArgs = (self, args) => {
        const built = buildArgs(args, rest);
        built.kwargs[selfName] = self;
        return built;
    };
    return {
        bind(self) {
            const typeArgsFor = (built) => buildTypeArgs(self, built.types, typeParams, classTypeParams);
            if (mode === 'sync') {
                return (...args) => {
                    const built = makeArgs(self, args);
                    const typeArgs = typeArgsFor(built);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(built.kwargs, { syncMode: true, callId, typeArgs });
                    const callCtxBinding = attachCallContext(built.ctx, built.signal, callId);
                    let resultBytes;
                    try {
                        resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null);
                    }
                    catch (error) {
                        throw wrapNativeError(error);
                    }
                    finally {
                        callCtxBinding.detach();
                    }
                    return decodeCallResult(resultBytes);
                };
            }
            if (mode === 'async') {
                return async (...args) => {
                    const built = makeArgs(self, args);
                    const typeArgs = typeArgsFor(built);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(built.kwargs, { callId, typeArgs });
                    const callCtxBinding = attachCallContext(built.ctx, built.signal, callId);
                    let resultBytes;
                    try {
                        resultBytes = await rt.callFunction(bamlFqn, argsProto, null, null);
                    }
                    catch (error) {
                        throw wrapNativeError(error);
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