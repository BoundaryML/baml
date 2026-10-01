import { _currentInvocation, _Invocation, _invoke, _invokeAsync, _withInvocation, type BamlType, type BamlTypeToken } from "__RUNTIME__";
import type * as Trace from "./vendor/trace/index.js";
import type { CancelToken } from "./baml/spawn/index.js";

export interface BamlOptions {
    readonly trace?: Trace.Options | Trace.ReservedSpan | null;
    readonly cancel?: CancelToken | null;
    readonly timeoutMs?: number | null;
    readonly signal?: AbortSignal | null;
}
export interface Invocation {
    readonly cancel: CancelToken;
    readonly signal: AbortSignal;
    run<T>(body: () => T): T;
}
export const invocation = {
    current: (): Invocation | null => _currentInvocation() as Invocation | null,
    withInvocation: <A extends unknown[], R>(body: (active: Invocation, ...args: A) => R): ((...args: A) => R) => _withInvocation(body as (active: _Invocation, ...args: A) => R),
};
export type InvocationCallOptions = {
    $baml?: BamlOptions | null;
    $types?: Record<string, BamlType | BamlTypeToken> | null;
};
export const invoke = _invoke as (target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvocationCallOptions) => unknown;
export const invokeAsync = _invokeAsync as (target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvocationCallOptions) => Promise<unknown>;
