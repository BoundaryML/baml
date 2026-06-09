// errors.ts — mirrors bridge_python/python_src/baml_py/errors.py
// Error types are encoded as prefixed strings in napi::Error messages.
// This module provides helpers to identify error types from native errors.

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
    constructor(message: string) {
        super(message);
        this.name = 'BamlInvalidArgumentError';
    }
}

export class BamlClientError extends BamlError {
    constructor(message: string) {
        super(message);
        this.name = 'BamlClientError';
    }
}

export class BamlCancelledError extends BamlError {
    constructor(message: string) {
        super(message);
        this.name = 'BamlCancelledError';
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

// Native errors are encoded as prefixed strings in napi::Error messages, e.g.
// `BamlError: BamlCancelledError: <detail>`. Match on the exact prefix rather
// than a substring so a user-supplied message that merely *contains* the words
// can't be misclassified.
const PREFIX_MAP: Array<[string, new (m: string) => BamlError]> = [
    ['BamlError: BamlCancelledError:', BamlCancelledError],
    ['BamlError: BamlInvalidArgumentError:', BamlInvalidArgumentError],
    ['BamlError: BamlClientError:', BamlClientError],
];

export function wrapNativeError(err: unknown): BamlError {
    if (!(err instanceof Error)) return new BamlError(String(err));
    const msg = err.message;
    for (const [prefix, Ctor] of PREFIX_MAP) {
        if (msg.startsWith(prefix)) return new Ctor(msg);
    }
    return new BamlError(msg);
}
