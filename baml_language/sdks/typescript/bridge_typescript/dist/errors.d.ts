/**
 * Structured detail carried by a thrown `BamlError` / `BamlPanic`, mirroring
 * `bridge_python`'s `BamlError(value, baml_trace=..., class_name=...)`.
 *
 * - `value`: the fully decoded thrown BAML value (a generated class instance
 *   when the FQN is mapped, else a plain object / primitive).
 * - `bamlTrace`: the pre-rendered `File "...", line N, in fn` frame strings
 *   from the BAML stack.
 * - `className`: the thrown value's BAML FQN when known (e.g.
 *   `baml.json.JsonParseError`).
 */
export interface BamlErrorDetail {
    value?: unknown;
    bamlTrace?: string[];
    className?: string;
}
export declare class BamlError extends Error {
    /** The decoded thrown BAML value, or `undefined` for SDK-internal errors. */
    readonly value: unknown;
    /** Pre-rendered BAML stack frames; empty for SDK-internal errors. */
    readonly bamlTrace: string[];
    /** The thrown value's BAML FQN, when known. */
    readonly className: string | undefined;
    constructor(message: string, detail?: BamlErrorDetail);
}
export declare class BamlInvalidArgumentError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail);
}
export declare class BamlClientError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail);
}
export declare class BamlCancelledError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail);
}
export declare class BamlAbortError extends Error {
    readonly reason: unknown;
    constructor(message: string, options?: {
        reason?: unknown;
    });
}
/**
 * Raised for SDK-setup failures and BAML-runtime panics — the Node analog of
 * `bridge_python`'s `BamlPanic`. The in-call panic path (`decodeCallResult`'s
 * `panic` arm) and the `getRuntime` not-initialized path both surface this,
 * except clean process-exit panics, which exit after flushing telemetry.
 */
export declare class BamlPanic extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail);
}
export declare function wrapNativeError(err: unknown): BamlError;
//# sourceMappingURL=errors.d.ts.map