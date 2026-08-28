/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// index.ts — mirrors bridge_python/python_src/baml_py/__init__.py
import { BamlRuntime, Collector as NativeCollector, cancelFunctionCall as nativeCancelFunctionCall, newFunctionCall as nativeNewFunctionCall, } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
import { installFlushOnExit } from './exit_hook.js';
import { wrapNativeError } from './errors.js';
import { attachCallContext } from './call_context.js';
export { BamlRuntime, BamlCallContext, BamlHandle, HostSpanManager, getRuntime, getBridgeRuntimeVersion, getToolchainVersion, getVersion, flushEvents, } from './native.js';
export { Timing, Usage } from './native.js';
export { _seedFunctionRefHandle, _seedGenericMediaHandle } from './native.js';
// Runtime-owned stdlib value classes. Exported under their `Baml*` names only;
// codegen aliases them as Image/Audio/Video/Pdf on re-export.
export { BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native.js';
// Stream wrapper. Exported as `BamlStream`; codegen aliases it as `Stream`.
export { BamlStream } from './stream.js';
export { BamlFunctionSpec } from './function_spec.js';
export { BamlPrompt, encodeCallArgs, decodeCallResult } from './proto.js';
export { CtxManager } from './ctx_manager.js';
// Codegen support: typemap + placeholder sentinel + free runtime initializer.
export { BamlTypeMap, setTypeMap, getTypeMap } from './typemap.js';
// Callable factories the generated SDK emits for every BAML function/method.
export { defineFunction, defineInstanceFunction, UNSET } from './define_function.js';
// Generic-type spelling for `$types` bindings on generic classes / calls.
export { BamlType, Never, lowerTypeToWireTy, reflectType } from './wire_ty.js';
/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts`:
 * `initializeRuntime("baml_src", _inlinedbaml.FILES)`. Thin wrapper over the
 * `BamlRuntime.initializeRuntime` factory (which sets the process-global
 * singleton reachable via `getRuntime()`).
 */
export function initializeRuntime(srcDir, files) {
    BamlRuntime.initializeRuntime(srcDir, files);
}
/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts` when
 * codegen embeds precompiled BAML bytecode.
 */
export function initializeRuntimeFromBytecode(bytecode, embeddedBamlToml) {
    BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytecode), embeddedBamlToml);
}
export { BamlAbortError, BamlError, BamlInvalidArgumentError, BamlClientError, BamlCancelledError, BamlPanic, wrapNativeError, } from './errors.js';
export function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
export function cancelFunctionCall(callId) {
    return nativeCancelFunctionCall(callId.toString());
}
import './unhandled_spawn.js';
export class FunctionResult {
    _value;
    constructor(value) {
        this._value = value;
    }
    result() {
        return this._value;
    }
    toString() {
        return `FunctionResult(${JSON.stringify(this._value)})`;
    }
}
export class FunctionLog {
    _inner;
    constructor(inner) { this._inner = inner; }
    get id() { return this._inner.id; }
    get functionName() { return this._inner.functionName; }
    get timing() { return this._inner.timing; }
    get usage() { return this._inner.usage; }
    get calls() { return this._inner.calls; }
    get tags() { return this._inner.tags; }
    // FIXME: Returns null for both "no serialized result" (bytes == null) and a legitimate
    // BAML null result (decodeCallResult returns null). Legacy engine/ had no result getter
    // on FunctionLog at all. bridge_python has the same ambiguity (None for both cases).
    // Leaving as-is for parity with bridge_python; narrow edge case in practice.
    get result() {
        const bytes = this._inner.result;
        if (bytes == null)
            return null;
        return decodeCallResult(bytes);
    }
}
export class Collector {
    _inner;
    constructor(name) { this._inner = new NativeCollector(name ?? null); }
    get name() { return this._inner.name; }
    get logs() {
        return this._inner.logs.map((l) => new FunctionLog(l));
    }
    get last() {
        const l = this._inner.last;
        return l ? new FunctionLog(l) : null;
    }
    get usage() { return this._inner.usage; }
    clear() { return this._inner.clear(); }
    id(functionLogId) {
        const l = this._inner.id(functionLogId);
        return l ? new FunctionLog(l) : null;
    }
    /** Internal: get native collector for passing to Rust */
    _native() { return this._inner; }
}
export function callFunctionSync(rt, functionName, kwargs, ctx, collectors, callCtx, operation = 'direct') {
    // Encode in sync mode so a host callable in the kwargs fast-fails
    // with a clear error instead of registering a tsfn and then hanging —
    // the sync path blocks the Node main thread on a tokio `block_on`,
    // starving libuv so the dispatch could never run.
    const callId = newFunctionCall();
    const argsProto = encodeCallArgs(kwargs, { syncMode: true, callId, functionName, operation });
    const callCtxBinding = attachCallContext(callCtx, callId);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes;
        try {
            resultBytes = rt.callFunctionSync(argsProto, ctx ?? null, nativeCollectors);
        }
        catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(decodeCallResult(resultBytes));
    }
    finally {
        callCtxBinding.detach();
    }
}
export async function callFunction(rt, functionName, kwargs, ctx, collectors, callCtx, operation = 'direct') {
    const callId = newFunctionCall();
    const argsProto = encodeCallArgs(kwargs, { callId, functionName, operation });
    const callCtxBinding = attachCallContext(callCtx, callId);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes;
        try {
            resultBytes = await rt.callFunction(argsProto, ctx ?? null, nativeCollectors);
        }
        catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(decodeCallResult(resultBytes));
    }
    finally {
        callCtxBinding.detach();
    }
}
// Register flush on process exit (single registration; see exit_hook.ts).
installFlushOnExit();
//# sourceMappingURL=index.js.map