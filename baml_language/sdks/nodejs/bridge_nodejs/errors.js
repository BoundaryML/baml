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
exports.BamlPanic = exports.BamlCancelledError = exports.BamlClientError = exports.BamlInvalidArgumentError = exports.BamlError = void 0;
exports.wrapNativeError = wrapNativeError;
class BamlError extends Error {
    constructor(message, detail) {
        super(message);
        this.name = 'BamlError';
        this.value = detail?.value;
        this.bamlTrace = detail?.bamlTrace ? [...detail.bamlTrace] : [];
        this.className = detail?.className;
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
/**
 * Raised for SDK-setup failures and BAML-runtime panics — the Node analog of
 * `bridge_python`'s `BamlPanic`. The in-call panic path (`decodeCallResult`'s
 * `panic` arm) and the `getRuntime` not-initialized path both surface this,
 * except clean process-exit panics, which exit after flushing telemetry.
 */
class BamlPanic extends BamlError {
    constructor(message, detail) {
        super(message, detail);
        this.name = 'BamlPanic';
    }
}
exports.BamlPanic = BamlPanic;
// Native errors are encoded as prefixed strings in napi::Error messages, e.g.
// `BamlError: BamlCancelledError: <detail>`. Match on the exact prefix rather
// than a substring so a user-supplied message that merely *contains* the words
// can't be misclassified.
const PREFIX_MAP = [
    ['BamlError: BamlCancelledError:', BamlCancelledError],
    ['BamlError: BamlInvalidArgumentError:', BamlInvalidArgumentError],
    ['BamlError: BamlClientError:', BamlClientError],
];
function wrapNativeError(err) {
    if (!(err instanceof Error))
        return new BamlError(String(err));
    const msg = err.message;
    for (const [prefix, Ctor] of PREFIX_MAP) {
        if (msg.startsWith(prefix))
            return new Ctor(msg);
    }
    return new BamlError(msg);
}
//# sourceMappingURL=errors.js.map