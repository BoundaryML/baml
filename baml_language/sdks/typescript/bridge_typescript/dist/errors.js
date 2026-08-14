/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// errors.ts — mirrors bridge_python/python_src/baml_py/errors.py
export class BamlError extends Error {
    /** The decoded thrown BAML value, or `undefined` for SDK-internal errors. */
    value;
    /** Pre-rendered BAML stack frames; empty for SDK-internal errors. */
    bamlTrace;
    /** The thrown value's BAML FQN, when known. */
    className;
    constructor(message, detail) {
        super(message);
        this.name = 'BamlError';
        this.value = detail?.value;
        this.bamlTrace = detail?.bamlTrace ? [...detail.bamlTrace] : [];
        this.className = detail?.className;
    }
}
export class BamlInvalidArgumentError extends BamlError {
    constructor(message, detail) {
        super(message, detail);
        this.name = 'BamlInvalidArgumentError';
    }
}
export class BamlClientError extends BamlError {
    constructor(message, detail) {
        super(message, detail);
        this.name = 'BamlClientError';
    }
}
export class BamlCancelledError extends BamlError {
    constructor(message, detail) {
        super(message, detail);
        this.name = 'BamlCancelledError';
    }
}
export class BamlAbortError extends Error {
    reason;
    constructor(message, options) {
        super(message);
        this.name = 'AbortError';
        this.reason = options?.reason;
    }
}
/**
 * Raised for SDK-setup failures and BAML-runtime panics — the Node analog of
 * `bridge_python`'s `BamlPanic`. The in-call panic path (`decodeCallResult`'s
 * `panic` arm) and the `getRuntime` not-initialized path both surface this,
 * except clean process-exit panics, which exit after flushing telemetry.
 */
export class BamlPanic extends BamlError {
    constructor(message, detail) {
        super(message, detail);
        this.name = 'BamlPanic';
    }
}
export function wrapNativeError(err) {
    if (err instanceof BamlError)
        return err;
    const message = err instanceof Error ? err.message : String(err);
    const code = err !== null && typeof err === 'object' && 'code' in err
        ? err.code
        : undefined;
    if (code === 'invalid_argument')
        return new BamlInvalidArgumentError(message);
    return new BamlClientError(message);
}
//# sourceMappingURL=errors.js.map