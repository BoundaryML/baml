/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
export declare class TraceUsageError extends TypeError {
}
type Body = (this: any, ...args: any[]) => any;
type Display = {
    readonly name?: string;
};
/** Wrap a host function, capturing outputs and errors by default.
 * Inputs require explicit options; pass trace.empty_span() for no value capture.
 */
export declare function instrument<F extends Body>(body: F): F;
export declare function instrument<F extends Body>(options: unknown, body: F, display?: Display): F;
export {};
//# sourceMappingURL=instrumentation.d.ts.map