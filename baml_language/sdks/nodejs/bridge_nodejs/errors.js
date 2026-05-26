/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// errors.ts — mirrors bridge_python/python_src/baml_py/errors.py
// Error types are encoded as prefixed strings in napi::Error messages.
// This module provides helpers to identify error types from native errors.
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlCancelledError = exports.BamlClientError = exports.BamlInvalidArgumentError = exports.BamlError = void 0;
exports.wrapNativeError = wrapNativeError;
class BamlError extends Error {
    constructor(message) {
        super(message);
        this.name = 'BamlError';
    }
}
exports.BamlError = BamlError;
class BamlInvalidArgumentError extends BamlError {
    constructor(message) {
        super(message);
        this.name = 'BamlInvalidArgumentError';
    }
}
exports.BamlInvalidArgumentError = BamlInvalidArgumentError;
class BamlClientError extends BamlError {
    constructor(message) {
        super(message);
        this.name = 'BamlClientError';
    }
}
exports.BamlClientError = BamlClientError;
class BamlCancelledError extends BamlError {
    constructor(message) {
        super(message);
        this.name = 'BamlCancelledError';
    }
}
exports.BamlCancelledError = BamlCancelledError;
function wrapNativeError(err) {
    if (err instanceof Error) {
        const msg = err.message;
        if (msg.includes('BamlCancelledError'))
            return new BamlCancelledError(msg);
        if (msg.includes('BamlInvalidArgumentError'))
            return new BamlInvalidArgumentError(msg);
        if (msg.includes('BamlClientError'))
            return new BamlClientError(msg);
        return new BamlError(msg);
    }
    return new BamlError(String(err));
}
//# sourceMappingURL=errors.js.map