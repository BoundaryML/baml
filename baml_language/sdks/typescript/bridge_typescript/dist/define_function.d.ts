/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { type FunctionOperation } from './proto.js';
export type Mode = 'sync' | 'async';
export type FunctionProjection = FunctionOperation;
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export declare const UNSET: unique symbol;
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
/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export declare function defineFunction(bamlFqn: string, mode: Mode, requiredParamNames: readonly string[], optionalParamNames?: readonly string[] | undefined, generics?: GenericParams | undefined, projection?: FunctionProjection): (...args: unknown[]) => unknown;
/**
 * Receiver-binding factory for instance methods. `paramNames[0]` is always
 * `"self"`. Codegen emits the binding as a class-field initializer
 * `m = defineInstanceFunction(...).bind(this) as () => R;`, so `.bind(self)`
 * captures the instance at construction time; the synthetic `self` param never
 * appears in the surface type.
 */
export declare function defineInstanceFunction(bamlFqn: string, mode: Mode, requiredParamNames: readonly string[], optionalParamNames?: readonly string[] | undefined, generics?: GenericParams | undefined, operation?: FunctionOperation): {
    bind(self: unknown): (...args: unknown[]) => unknown;
};
//# sourceMappingURL=define_function.d.ts.map