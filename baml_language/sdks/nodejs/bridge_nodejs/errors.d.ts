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
export declare function wrapNativeError(err: unknown): BamlError;
//# sourceMappingURL=errors.d.ts.map