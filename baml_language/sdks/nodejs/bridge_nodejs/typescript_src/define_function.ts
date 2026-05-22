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

import { getRuntime } from './native';
import { encodeCallArgs, decodeCallResult } from './proto';
import { wrapNativeError } from './errors';

export type FunctionMode = 'sync' | 'async';

function buildKwargs(
    paramNames: readonly string[],
    args: unknown[],
): Record<string, unknown> {
    // BAML calls are keyword-only per the spec. The codegen-emitted call
    // site can pass either:
    //   - A single object containing all kwargs (the canonical form), or
    //   - Positional args (one per param), zipped against paramNames.
    if (args.length === 1 && isPlainObject(args[0])) {
        return { ...(args[0] as Record<string, unknown>) };
    }
    const kwargs: Record<string, unknown> = {};
    for (let i = 0; i < args.length; i++) {
        const name = paramNames[i];
        if (name === undefined) {
            throw new TypeError(
                `Too many positional arguments for function: expected ${paramNames.length}, got ${args.length}`,
            );
        }
        kwargs[name] = args[i];
    }
    return kwargs;
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
    if (v == null || typeof v !== 'object') return false;
    const proto = Object.getPrototypeOf(v);
    return proto === Object.prototype || proto === null;
}

export function defineFunction(
    bamlFqn: string,
    mode: 'sync',
    paramNames: readonly string[],
): (...args: unknown[]) => unknown;
export function defineFunction(
    bamlFqn: string,
    mode: 'async',
    paramNames: readonly string[],
): (...args: unknown[]) => Promise<unknown>;
export function defineFunction(
    bamlFqn: string,
    mode: FunctionMode,
    paramNames: readonly string[],
): (...args: unknown[]) => unknown {
    if (mode === 'sync') {
        return (...args: unknown[]): unknown => {
            const kwargs = buildKwargs(paramNames, args);
            const argsProto = encodeCallArgs(kwargs);
            try {
                const resultBytes = getRuntime().callFunctionSync(
                    bamlFqn,
                    argsProto,
                    null,
                    null,
                    null,
                );
                return decodeCallResult(resultBytes);
            } catch (err) {
                throw wrapNativeError(err);
            }
        };
    }
    return async (...args: unknown[]): Promise<unknown> => {
        const kwargs = buildKwargs(paramNames, args);
        const argsProto = encodeCallArgs(kwargs);
        try {
            const resultBytes = await getRuntime().callFunction(
                bamlFqn,
                argsProto,
                null,
                null,
                null,
            );
            return decodeCallResult(resultBytes);
        } catch (err) {
            throw wrapNativeError(err);
        }
    };
}
