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

// Bridge errors arrive prefixed with `BamlError: <Subclass>:` (see
// bridge_nodejs/src/errors.rs). Match the prefix exactly to avoid
// over-matching user messages that incidentally mention a class name.
const PREFIX_MAP: Array<[string, new (m: string) => BamlError]> = [
    ['BamlError: BamlCancelledError:', BamlCancelledError],
    ['BamlError: BamlInvalidArgumentError:', BamlInvalidArgumentError],
    ['BamlError: BamlClientError:', BamlClientError],
];

export function wrapNativeError(err: unknown): BamlError {
    if (err instanceof BamlError) return err;
    // napi-rs Errors may not satisfy `instanceof Error` across realm
    // boundaries; duck-type on `.message` instead.
    const msg = typeof err === 'object' && err !== null && typeof (err as { message?: unknown }).message === 'string'
        ? (err as { message: string }).message
        : String(err);
    for (const [prefix, Ctor] of PREFIX_MAP) {
        if (msg.startsWith(prefix)) return new Ctor(msg);
    }
    return new BamlError(msg);
}
