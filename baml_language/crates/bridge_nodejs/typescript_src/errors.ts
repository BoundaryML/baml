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

export function wrapNativeError(err: unknown): BamlError {
    if (err instanceof Error) {
        const msg = err.message;
        if (msg.includes('BamlCancelledError')) return new BamlCancelledError(msg);
        if (msg.includes('BamlInvalidArgumentError')) return new BamlInvalidArgumentError(msg);
        if (msg.includes('BamlClientError')) return new BamlClientError(msg);
        return new BamlError(msg);
    }
    return new BamlError(String(err));
}
