/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
export declare class BamlError extends Error {
    constructor(message: string);
}
export declare class BamlInvalidArgumentError extends BamlError {
    constructor(message: string);
}
export declare class BamlClientError extends BamlError {
    constructor(message: string);
}
export declare class BamlCancelledError extends BamlError {
    constructor(message: string);
}
/**
 * Raised for SDK-setup failures and BAML-runtime panics — the Node analog of
 * `bridge_python`'s `BamlPanic`. The in-call panic path (`decodeCallResult`'s
 * `panic` arm) and the `getRuntime` not-initialized path both surface this,
 * except clean process-exit panics, which exit after flushing telemetry.
 */
export declare class BamlPanic extends BamlError {
    constructor(message: string);
}
export declare function wrapNativeError(err: unknown): BamlError;
//# sourceMappingURL=errors.d.ts.map