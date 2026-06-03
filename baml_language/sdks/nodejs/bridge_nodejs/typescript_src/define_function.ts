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

export type Mode = 'sync' | 'async';

/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export const UNSET: unique symbol = Symbol('baml.UNSET');

function buildKwargs(
    args: unknown[],
    paramNames: readonly string[],
    requiredPositionalCount?: number,
): Record<string, unknown> {
    const positionalLimit = requiredPositionalCount ?? paramNames.length;
    if (args.length > positionalLimit) {
        throw new TypeError(
            `got ${args.length} positional arguments but only ${positionalLimit} positional ` +
            `parameter names (${JSON.stringify(paramNames.slice(0, positionalLimit))})`,
        );
    }
    const built: Record<string, unknown> = {};
    for (let i = 0; i < args.length && i < paramNames.length; i++) {
        if (args[i] === UNSET) continue;
        built[paramNames[i]] = args[i];
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
    paramNames: readonly string[],
    requiredPositionalCount?: number,
): (...args: unknown[]) => unknown {
    const names = [...paramNames];
    if (mode === 'sync') {
        return (...args: unknown[]): unknown => {
            const merged = buildKwargs(args, names, requiredPositionalCount);
            const rt = getRuntime();
            const argsProto = encodeCallArgs(merged, /* syncMode */ true);
            const resultBytes = rt.callFunctionSync(bamlFqn, argsProto, null, null, null);
            return decodeCallResult(resultBytes);
        };
    }
    if (mode === 'async') {
        return async (...args: unknown[]): Promise<unknown> => {
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
export function defineInstanceFunction(
    bamlFqn: string,
    mode: Mode,
    paramNames: readonly string[],
): { bind(self: unknown): (...args: unknown[]) => unknown } {
    const names = [...paramNames];
    const selfName = names[0] ?? 'self';
    const rest = names.slice(1);

    const makeKwargs = (self: unknown, args: unknown[]): Record<string, unknown> => {
        const merged = buildKwargs(args, rest);
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
