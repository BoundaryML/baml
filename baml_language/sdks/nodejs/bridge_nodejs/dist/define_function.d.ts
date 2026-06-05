/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
export type Mode = 'sync' | 'async';
/** Sentinel for "argument not supplied" so optional kwargs can be skipped. */
export declare const UNSET: unique symbol;
/**
 * Factory for a free function or static method binding. Returns a callable
 * that maps positional args to kwargs, encodes, calls the runtime, and decodes.
 * `sync` returns the decoded value; `async` returns a `Promise` of it.
 */
export declare function defineFunction(bamlFqn: string, mode: Mode, requiredParamNames: readonly string[], optionalParamNames?: readonly string[] | undefined): (...args: unknown[]) => unknown;
/**
 * Receiver-binding factory for instance methods. `paramNames[0]` is always
 * `"self"`. Codegen emits the binding as a class-field initializer
 * `m = defineInstanceFunction(...).bind(this) as () => R;`, so `.bind(self)`
 * captures the instance at construction time; the synthetic `self` param never
 * appears in the surface type.
 */
export declare function defineInstanceFunction(bamlFqn: string, mode: Mode, requiredParamNames: readonly string[], optionalParamNames?: readonly string[] | undefined): {
    bind(self: unknown): (...args: unknown[]) => unknown;
};
//# sourceMappingURL=define_function.d.ts.map