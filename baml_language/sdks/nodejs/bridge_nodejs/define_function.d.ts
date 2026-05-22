/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
export type FunctionMode = 'sync' | 'async';
export declare function defineFunction(bamlFqn: string, mode: 'sync', paramNames: readonly string[]): (...args: unknown[]) => unknown;
export declare function defineFunction(bamlFqn: string, mode: 'async', paramNames: readonly string[]): (...args: unknown[]) => Promise<unknown>;
//# sourceMappingURL=define_function.d.ts.map