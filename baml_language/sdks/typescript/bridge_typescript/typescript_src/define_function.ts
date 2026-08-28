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

import {
    BamlCallContext,
    getRuntime,
    newFunctionCall as nativeNewFunctionCall,
} from './native.js';
import { encodeCallArgs, decodeCallResult, type FunctionOperation } from './proto.js';
import { attachCallContext } from './call_context.js';
import { baml_bridge } from './proto/baml_cffi.js';
import { BamlType, lowerTypeToWireTy, type BamlTypeToken } from './wire_ty.js';

export type Mode = 'sync' | 'async';
export type FunctionProjection = FunctionOperation;

/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET: unique symbol = Symbol('baml.UNSET');

/**
 * Generic-binding contract for a callable. `typeParams` are the callee's OWN
 * `<...>` param names (bound by the caller's `$types` option); `classTypeParams`
 * are the enclosing generic class's params (bound from the `self` receiver's
 * `$types` field). Mirrors `define_function`'s `type_params` /
 * `class_type_params` kwargs in the Python SDK. Omitted/empty ⇒ non-generic.
 */
export interface GenericParams {
    typeParams?: readonly string[];
    classTypeParams?: readonly string[];
}

interface BuiltArgs {
    kwargs: Record<string, unknown>;
    ctx?: BamlCallContext;
    /** The `$types` call option (TypeVar bindings), captured from the trailing
     * options object. `undefined` when not supplied. */
    types?: unknown;
}

type HostTypeToken = BamlType | BamlTypeToken;
type WireTypeArg = [string, baml_bridge.cffi.v1.IBamlTy | BamlType];

/**
 * Resolve the caller's `$types` option onto the callee's own generic params, in
 * declaration order. Omitted bindings are inferred by the engine; supplied
 * names must belong to the callee.
 */
function resolveTypesOption(typesOpt: unknown, typeParams: readonly string[]): Array<[string, HostTypeToken]> {
    if (typeParams.length === 0) {
        if (typesOpt !== undefined && typesOpt !== null) {
            throw new TypeError(
                '$types is not accepted here: this function/method declares no generic type ' +
                'parameters of its own',
            );
        }
        return [];
    }
    if (typesOpt === undefined || typesOpt === null) {
        return [];
    }
    const example = `{ ${JSON.stringify(typeParams[0])}: 'int' }`;
    if (typeof typesOpt !== 'object' || Array.isArray(typesOpt)) {
        throw new TypeError(
            `$types must be an object mapping type-parameter names to types (e.g. ` +
            `$types: ${example}); got ${typeof typesOpt}`,
        );
    }
    const obj = typesOpt as Record<string, HostTypeToken | undefined>;
    const extra = Object.keys(obj).filter((k) => !typeParams.includes(k));
    if (extra.length) {
        throw new TypeError(
            `$types has unknown type parameter(s) ${JSON.stringify(extra)}; expected ` +
            `${JSON.stringify(typeParams)}.`,
        );
    }
    return typeParams
        .filter((name) => obj[name] !== undefined)
        .map((name) => [name, obj[name] as HostTypeToken]);
}

/** The `$types` field of a generic receiver instance (its class TypeVar
 * bindings), or `undefined` when the receiver carries none. The TS analog of
 * Python's `pydantic_instance_type_args(self)`. */
function receiverTypes(self: unknown): Record<string, HostTypeToken> | undefined {
    if (self != null && typeof self === 'object') {
        const t = (self as { $types?: unknown }).$types;
        if (t != null && typeof t === 'object') {
            return t as Record<string, HostTypeToken>;
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
function buildTypeArgs(
    self: unknown,
    typesOpt: unknown,
    typeParams: readonly string[],
    classTypeParams: readonly string[],
): WireTypeArg[] {
    const wire: WireTypeArg[] = [];
    const classTypes = classTypeParams.length ? receiverTypes(self) : undefined;
    if (classTypes) {
        for (const name of classTypeParams) {
            const token = classTypes[name];
            if (token !== undefined) {
                wire.push([name, token instanceof BamlType ? token : lowerTypeToWireTy(token)]);
            }
        }
    }
    const resolved = resolveTypesOption(typesOpt, typeParams);
    if (resolved.length > 0) {
        if (classTypeParams.length && !classTypes) {
            // The method's own params sit after the class prefix in De Bruijn
            // order; without recovered class args we can't position them.
            throw new TypeError(
                '$types on a generic method requires a generic receiver carrying its class type ' +
                'args (a `$types` field on the instance)',
            );
        }
        for (const [name, token] of resolved) {
            wire.push([name, token instanceof BamlType ? token : lowerTypeToWireTy(token)]);
        }
    }
    return wire;
}

function newFunctionCall(): bigint {
    return BigInt(nativeNewFunctionCall());
}

function buildArgs(
    args: unknown[],
    requiredParamNames: readonly string[],
    optionalParamNames: readonly string[],
): BuiltArgs {
    const positionalLimit = requiredParamNames.length;
    if (args.length > positionalLimit + 1) {
        throw new TypeError(
            `got ${args.length} positional arguments but only ${positionalLimit} positional ` +
            `parameter names (${JSON.stringify(requiredParamNames)})`,
        );
    }
    const built: Record<string, unknown> = {};
    for (let i = 0; i < args.length && i < positionalLimit; i++) {
        if (args[i] === UNSET) continue;
        built[requiredParamNames[i]] = args[i];
    }
    let ctx: BamlCallContext | undefined;
    let types: unknown;
    if (args.length > positionalLimit) {
        const opts = args[positionalLimit];
        if (opts === undefined || opts === UNSET) {
            return { kwargs: built };
        }
        if (opts === null || Array.isArray(opts) || typeof opts !== 'object') {
            throw new TypeError('optional arguments must be passed as an object');
        }
        const optionNames = new Set(optionalParamNames);
        for (const [key, value] of Object.entries(opts as Record<string, unknown>)) {
            if (key === '$ctx') {
                if (value !== undefined && value !== UNSET) {
                    ctx = value as BamlCallContext;
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
            if (!optionNames.has(key)) {
                throw new TypeError(`unknown optional argument ${JSON.stringify(key)}`);
            }
            if (value === undefined || value === UNSET) continue;
            built[key] = value;
        }
    }
    return { kwargs: built, ctx, types };
}

/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export function defineFunction(
    bamlFqn: string,
    mode: Mode,
    requiredParamNames: readonly string[],
    optionalParamNames?: readonly string[] | undefined,
    generics?: GenericParams | undefined,
    projection: FunctionProjection = 'direct',
): (...args: unknown[]) => unknown {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    // A free function / static method binds only its OWN generic params (a
    // generic receiver is never in play here), so `classTypeParams` is unused.
    const typeParams = generics?.typeParams ?? [];
    if (projection !== 'direct' && projection !== 'spec' && projection !== 'stream') {
        throw new TypeError(`unknown function projection ${JSON.stringify(projection)}`);
    }
    const isGeneric = typeParams.length > 0;
    // Eagerly reject `$types` on a non-generic call, matching the generic path's
    // strict binding contract (mirrors Python's `is_generic` gate).
    const typeArgsFor = (built: BuiltArgs): WireTypeArg[] =>
        isGeneric ? buildTypeArgs(undefined, built.types, typeParams, []) : [];
    if (mode === 'sync') {
        return (...args: unknown[]): unknown => {
            const built = buildArgs(args, requiredNames, optionNames);
            const typeArgs = typeArgsFor(built);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(built.kwargs, {
                syncMode: true,
                callId,
                typeArgs,
                functionName: bamlFqn,
                operation: projection,
            });
            const callCtxBinding = attachCallContext(built.ctx, callId);
            try {
                const resultBytes = rt.callFunctionSync(argsProto, null, null);
                return decodeCallResult(resultBytes);
            } finally {
                callCtxBinding.detach();
            }
        };
    }
    if (mode === 'async') {
        return async (...args: unknown[]): Promise<unknown> => {
            const built = buildArgs(args, requiredNames, optionNames);
            const typeArgs = typeArgsFor(built);
            const rt = getRuntime();
            const callId = newFunctionCall();
            const argsProto = encodeCallArgs(built.kwargs, {
                callId,
                typeArgs,
                functionName: bamlFqn,
                operation: projection,
            });
            const callCtxBinding = attachCallContext(built.ctx, callId);
            try {
                const resultBytes = await rt.callFunction(argsProto, null, null);
                return decodeCallResult(resultBytes);
            } finally {
                callCtxBinding.detach();
            }
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
export function defineInstanceFunction(
    bamlFqn: string,
    mode: Mode,
    requiredParamNames: readonly string[],
    optionalParamNames?: readonly string[] | undefined,
    generics?: GenericParams | undefined,
    operation: FunctionOperation = 'direct',
): { bind(self: unknown): (...args: unknown[]) => unknown } {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    const selfName = requiredNames[0] ?? 'self';
    const rest = requiredNames.slice(1);
    // An instance method binds its own `<...>` params (caller's `$types`) AND
    // the enclosing class's params, recovered from the `self` receiver's
    // `$types` field. Mirrors Python's `class_type_params` for instance methods.
    const typeParams = generics?.typeParams ?? [];
    const classTypeParams = generics?.classTypeParams ?? [];
    const isGeneric = typeParams.length > 0 || classTypeParams.length > 0;

    const makeArgs = (self: unknown, args: unknown[]): BuiltArgs => {
        const built = buildArgs(args, rest, optionNames);
        built.kwargs[selfName] = self;
        return built;
    };

    return {
        bind(self: unknown): (...args: unknown[]) => unknown {
            const typeArgsFor = (built: BuiltArgs): WireTypeArg[] =>
                isGeneric ? buildTypeArgs(self, built.types, typeParams, classTypeParams) : [];
            if (mode === 'sync') {
                return (...args: unknown[]): unknown => {
                    const built = makeArgs(self, args);
                    const typeArgs = typeArgsFor(built);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(built.kwargs, { syncMode: true, callId, typeArgs, functionName: bamlFqn, operation });
                    const callCtxBinding = attachCallContext(built.ctx, callId);
                    try {
                        const resultBytes = rt.callFunctionSync(argsProto, null, null);
                        return decodeCallResult(resultBytes);
                    } finally {
                        callCtxBinding.detach();
                    }
                };
            }
            if (mode === 'async') {
                return async (...args: unknown[]): Promise<unknown> => {
                    const built = makeArgs(self, args);
                    const typeArgs = typeArgsFor(built);
                    const rt = getRuntime();
                    const callId = newFunctionCall();
                    const argsProto = encodeCallArgs(built.kwargs, { callId, typeArgs, functionName: bamlFqn, operation });
                    const callCtxBinding = attachCallContext(built.ctx, callId);
                    try {
                        const resultBytes = await rt.callFunction(argsProto, null, null);
                        return decodeCallResult(resultBytes);
                    } finally {
                        callCtxBinding.detach();
                    }
                };
            }
            throw new Error(`mode must be 'sync' or 'async', got ${JSON.stringify(mode)}`);
        },
    };
}
