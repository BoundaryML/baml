// errors.ts — mirrors bridge_python/python_src/baml_py/errors.py

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

export class BamlError extends Error {
    /** The decoded thrown BAML value, or `undefined` for SDK-internal errors. */
    readonly value: unknown;
    /** Pre-rendered BAML stack frames; empty for SDK-internal errors. */
    readonly bamlTrace: string[];
    /** The thrown value's BAML FQN, when known. */
    readonly className: string | undefined;

    constructor(message: string, detail?: BamlErrorDetail) {
        super(message);
        this.name = 'BamlError';
        this.value = detail?.value;
        this.bamlTrace = detail?.bamlTrace ? [...detail.bamlTrace] : [];
        this.className = detail?.className;
    }
}

export class BamlInvalidArgumentError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail) {
        super(message, detail);
        this.name = 'BamlInvalidArgumentError';
    }
}

export class BamlClientError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail) {
        super(message, detail);
        this.name = 'BamlClientError';
    }
}

export class BamlCancelledError extends BamlError {
    constructor(message: string, detail?: BamlErrorDetail) {
        super(message, detail);
        this.name = 'BamlCancelledError';
    }
}

export class BamlAbortError extends Error {
    readonly reason: unknown;

    constructor(message: string, options?: { reason?: unknown }) {
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
    constructor(message: string, detail?: BamlErrorDetail) {
        super(message, detail);
        this.name = 'BamlPanic';
    }
}

export function wrapNativeError(err: unknown): BamlError {
    if (err instanceof BamlError) return err;
    const message = err instanceof Error ? err.message : String(err);
    const code = err !== null && typeof err === 'object' && 'code' in err
        ? (err as { code?: unknown }).code
        : undefined;
    if (code === 'invalid_argument') return new BamlInvalidArgumentError(message);
    return new BamlClientError(message);
}
