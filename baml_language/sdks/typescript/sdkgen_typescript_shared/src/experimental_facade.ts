/** Experimental host APIs; these may change independently of the stable SDK. */
import { _invoke, _invokeAsync, type BamlType, type BamlTypeToken } from "__RUNTIME__";
import type { BamlOptions } from "./_invocation.js";

export type InvokeOptions = {
    $baml?: BamlOptions | null;
    $types?: Record<string, BamlType | BamlTypeToken> | null;
};
export const invoke = _invoke as (target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions) => unknown;
export const invokeAsync = _invokeAsync as (target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions) => Promise<unknown>;
