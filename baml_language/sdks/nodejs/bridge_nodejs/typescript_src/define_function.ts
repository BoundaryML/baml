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

export type Mode = 'sync' | 'async';

/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET: unique symbol = Symbol('baml.UNSET');

function buildKwargs(
    args: unknown[],
    requiredParamNames: readonly string[],
    optionalParamNames: readonly string[],
): Record<string, unknown> {
    const positionalLimit = requiredParamNames.length;
    const hasOpts = optionalParamNames.length > 0;
    if (args.length > positionalLimit + (hasOpts ? 1 : 0)) {
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
    if (hasOpts && args.length > positionalLimit) {
        const opts = args[positionalLimit];
        if (opts === undefined || opts === UNSET) {
            return built;
        }
        if (opts === null || Array.isArray(opts) || typeof opts !== 'object') {
            throw new TypeError('optional arguments must be passed as an object');
        }
        const optionNames = new Set(optionalParamNames);
        for (const [key, value] of Object.entries(opts as Record<string, unknown>)) {
            if (!optionNames.has(key)) {
                throw new TypeError(`unknown optional argument ${JSON.stringify(key)}`);
            }
            if (value === undefined || value === UNSET) continue;
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
export function defineFunction(
    bamlFqn: string,
    mode: Mode,
    requiredParamNames: readonly string[],
    optionalParamNames?: readonly string[] | undefined,
): (...args: unknown[]) => unknown {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    if (mode === 'sync') {
        return (...args: unknown[]): unknown => {
            const merged = buildKwargs(args, requiredNames, optionNames);
            const rt = getRuntime();
            const argsProto = encodeCallArgs(merged, /* syncMode */ true);
            const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args: unknown[]): Promise<unknown> => {
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
export function defineInstanceFunction(
    bamlFqn: string,
    mode: Mode,
    requiredParamNames: readonly string[],
    optionalParamNames?: readonly string[] | undefined,
): { bind(self: unknown): (...args: unknown[]) => unknown } {
    const requiredNames = [...requiredParamNames];
    const optionNames = [...(optionalParamNames ?? [])];
    const selfName = requiredNames[0] ?? 'self';
    const rest = requiredNames.slice(1);

    const makeKwargs = (self: unknown, args: unknown[]): Record<string, unknown> => {
        const merged = buildKwargs(args, rest, optionNames);
        merged[selfName] = self;
        return merged;
    };

    return {
        bind(self: unknown): (...args: unknown[]) => unknown {
            if (mode === 'sync') {
                return (...args: unknown[]): unknown => {
                    const merged = makeKwargs(self, args);
                    const rt = getRuntime();
                    const argsProto = encodeCallArgs(merged, /* syncMode */ true);
                    const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
                    return decodeCallResult(resultBytes);
                };
            }
            if (mode === 'async') {
                return async (...args: unknown[]): Promise<unknown> => {
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
