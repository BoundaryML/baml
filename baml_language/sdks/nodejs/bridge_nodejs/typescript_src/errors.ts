// errors.ts — mirrors bridge_python/python_src/baml_py/errors.py
// Error types are encoded as prefixed strings in napi::Error messages.
// This module provides helpers to identify error types from native errors.

export class BamlError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'BamlError';
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
    constructor(message: string) {
        super(message);
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
