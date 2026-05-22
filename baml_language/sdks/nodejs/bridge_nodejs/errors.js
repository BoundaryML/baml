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
// Bridge errors arrive prefixed with `BamlError: <Subclass>:` (see
// bridge_nodejs/src/errors.rs). Match the prefix exactly to avoid
// over-matching user messages that incidentally mention a class name.
const PREFIX_MAP = [
    ['BamlError: BamlCancelledError:', BamlCancelledError],
    ['BamlError: BamlInvalidArgumentError:', BamlInvalidArgumentError],
    ['BamlError: BamlClientError:', BamlClientError],
];
function wrapNativeError(err) {
    if (err instanceof BamlError)
        return err;
    // napi-rs Errors may not satisfy `instanceof Error` across realm
    // boundaries; duck-type on `.message` instead.
    const msg = typeof err === 'object' && err !== null && typeof err.message === 'string'
        ? err.message
        : String(err);
    for (const [prefix, Ctor] of PREFIX_MAP) {
        if (msg.startsWith(prefix))
            return new Ctor(msg);
    }
    return new BamlError(msg);
}
//# sourceMappingURL=errors.js.map