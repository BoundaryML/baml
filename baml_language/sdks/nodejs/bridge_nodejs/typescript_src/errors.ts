// errors.ts — mirrors bridge_python/src/baml_bridge/errors.py

import { getTypeMap } from './typemap.js';

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

const SDK_PANIC_CLASS = 'baml.panics.SdkPanic';

/**
 * Strip the historical napi-side error-class decorations from the message.
 * They predate structured BAML errors and are presentation text only: setup
 * failures always surface publicly as `BamlPanic(SdkPanic)`.
 */
function nativeErrorMessage(err: unknown): string {
    let message = err instanceof Error ? err.message : String(err);
    message = message.replace(/^BamlError:\s*/, '');
    message = message.replace(/^Baml(?:InvalidArgument|Client)Error:\s*/, '');
    return message || String(err);
}

/**
 * Build the Node equivalent of Python's `make_sdk_panic(message)`.
 *
 * Once a generated SDK has installed its typemap, `.value` is an actual
 * generated `baml.panics.SdkPanic` instance. During early import/setup the
 * typemap may not exist yet, so construction deliberately falls back to the
 * plain message string instead of masking the original failure.
 */
export function makeSdkPanic(message: string): BamlPanic {
    let value: unknown = message;
    try {
        const ctor = getTypeMap().getClass(SDK_PANIC_CLASS);
        if (typeof ctor !== 'function') {
            throw new TypeError(`${SDK_PANIC_CLASS} did not resolve to a constructor`);
        }
        value = new (ctor as new (init: { message: string }) => unknown)({ message });
    } catch {
        // The typemap is unavailable before generated SDK initialization (and
        // in the bare bridge package). Match Python by retaining the string.
    }

    return new BamlPanic(
        `baml panic: ${SDK_PANIC_CLASS}: ${message}`,
        { value, className: SDK_PANIC_CLASS },
    );
}

/**
 * Map a thrown napi error from a handle-returning/setup boundary to the
 * structured public panic contract. This helper must wrap only the native
 * call itself; decoder errors and rehydrated host exceptions must bypass it.
 */
export function wrapNativeError(err: unknown): BamlError {
    // Defensive idempotence for callers with a slightly wider catch block.
    if (err instanceof BamlError) return err;
    return makeSdkPanic(nativeErrorMessage(err));
}
