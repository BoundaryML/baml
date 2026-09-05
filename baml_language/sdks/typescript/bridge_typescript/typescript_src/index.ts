import { typeMapForRuntime, withTypeMap } from './typemap.js';
// index.ts — mirrors bridge_python/python_src/baml_py/__init__.py

import {
    BamlRuntime,
    BamlHandle,
    BamlCallContext,
    HostSpanManager,
    Collector as NativeCollector,
    FunctionLog as NativeFunctionLog,
    Timing,
    Usage,
    LLMCall,
    cancelFunctionCall as nativeCancelFunctionCall,
    newFunctionCall as nativeNewFunctionCall,
    releaseFunctionCall as nativeReleaseFunctionCall,
} from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
import { installFlushOnExit } from './exit_hook.js';
import { wrapNativeError } from './errors.js';
import { attachCallContext } from './call_context.js';

export {
    BamlRuntime,
    BamlCallContext,
    BamlHandle,
    HostSpanManager,
    getRuntime,
    getBridgeRuntimeVersion,
    getToolchainVersion,
    getVersion,
    flushEvents,
} from './native.js';
export { Timing, Usage, LLMCall } from './native.js';
export { _seedFunctionRefHandle, _seedGenericMediaHandle } from './native.js';
// Runtime-owned stdlib value classes. Exported under their `Baml*` names only;
// codegen aliases them as Image/Audio/Video/Pdf on re-export.
export { BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native.js';
// Stream wrapper. Exported as `BamlStream`; codegen aliases it as `Stream`.
export { BamlStream } from './stream.js';
export { BamlFunctionSpec } from './function_spec.js';
export type { BamlFunctionSpecBuildRequestOptions, BamlFunctionSpecCallOptions } from './function_spec.js';
export { BamlPrompt, encodeCallArgs, decodeCallResult } from './proto.js';
export type { BamlPromptCallOptions, BamlPromptMessage } from './proto.js';
export { CtxManager } from './ctx_manager.js';
// Codegen support: typemap + placeholder sentinel + free runtime initializer.
export { BamlTypeMap, setTypeMap, getTypeMap } from './typemap.js';
// Callable factories the generated SDK emits for every BAML function/method.
export { defineFunction, defineInstanceFunction, UNSET } from './define_function.js';
export type { GenericParams } from './define_function.js';
// Generic-type spelling for `$types` bindings on generic classes / calls.
export { BamlType, Never, lowerTypeToWireTy, reflectType } from './wire_ty.js';
export type { BamlTypeMetadata, BamlTypeToken, BamlPrimitiveToken, BamlClassCtor, BamlInterfaceToken } from './wire_ty.js';

/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts`:
 * `initializeRuntime("baml_src", _inlinedbaml.FILES)`. Thin wrapper over the
 * `BamlRuntime.initializeRuntime` factory (which sets the process-global
 * singleton reachable via `getRuntime()`).
 */
export function initializeRuntime(srcDir: string, files: Record<string, string>): BamlRuntime {
    return BamlRuntime.initializeRuntime(srcDir, files);
}

/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts` when
 * codegen embeds precompiled BAML bytecode.
 */
export function initializeRuntimeFromBytecode(bytecode: Buffer | Uint8Array, embeddedBamlToml?: string, runtimeKey?: bigint): BamlRuntime {
    return BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytecode), embeddedBamlToml, runtimeKey);
}
export {
    BamlAbortError,
    BamlError,
    BamlInvalidArgumentError,
    BamlClientError,
    BamlCancelledError,
    BamlPanic,
    wrapNativeError,
} from './errors.js';

export function newFunctionCall(): bigint {
    return BigInt(nativeNewFunctionCall());
}

/** Release an allocated ID when no call will be dispatched. */
export function releaseFunctionCall(callId: bigint): void {
    nativeReleaseFunctionCall(callId.toString());
}

export function cancelFunctionCall(callId: bigint): boolean {
    return nativeCancelFunctionCall(callId.toString());
}

import './unhandled_spawn.js';

export class FunctionResult {
    private _value: unknown;

    constructor(value: unknown) {
        this._value = value;
    }

    result(): unknown {
        return this._value;
    }

    toString(): string {
        return `FunctionResult(${JSON.stringify(this._value)})`;
    }
}

export class FunctionLog {
    private _inner: NativeFunctionLog;
    constructor(inner: NativeFunctionLog) { this._inner = inner; }
    get id(): string { return this._inner.id; }
    get functionName(): string { return this._inner.functionName; }
    get timing(): Timing { return this._inner.timing; }
    get usage(): Usage { return this._inner.usage; }
    get calls(): LLMCall[] { return this._inner.calls; }
    get tags(): Record<string, string> { return this._inner.tags; }
    // FIXME: Returns null for both "no serialized result" (bytes == null) and a legitimate
    // BAML null result (decodeCallResult returns null). Legacy engine/ had no result getter
    // on FunctionLog at all. bridge_python has the same ambiguity (None for both cases).
    // Leaving as-is for parity with bridge_python; narrow edge case in practice.
    get result(): unknown {
        const bytes = this._inner.result;
        if (bytes == null) return null;
        return decodeCallResult(bytes);
    }
}

export class Collector {
    private _inner: NativeCollector;
    constructor(name?: string) { this._inner = new NativeCollector(name ?? null); }
    get name(): string { return this._inner.name; }
    get logs(): FunctionLog[] {
        return this._inner.logs.map((l: NativeFunctionLog) => new FunctionLog(l));
    }
    get last(): FunctionLog | null {
        const l = this._inner.last;
        return l ? new FunctionLog(l) : null;
    }
    get usage(): Usage { return this._inner.usage; }
    clear(): number { return this._inner.clear(); }
    id(functionLogId: string): FunctionLog | null {
        const l = this._inner.id(functionLogId);
        return l ? new FunctionLog(l) : null;
    }
    /** Internal: get native collector for passing to Rust */
    _native(): NativeCollector { return this._inner; }
}

export function callFunctionSync(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    ctx?: HostSpanManager,
    collectors?: Collector[],
    callCtx?: BamlCallContext,
): FunctionResult {
    // Encode in sync mode so a host callable in the kwargs fast-fails
    // with a clear error instead of registering a tsfn and then hanging —
    // the sync path blocks the Node main thread on a tokio `block_on`,
    // starving libuv so the dispatch could never run.
    const typeMap = typeMapForRuntime(rt);
    const callId = newFunctionCall();
    const argsProto = withTypeMap(typeMap, () => encodeCallArgs(kwargs, { syncMode: true, callId, functionName }));
    const callCtxBinding = attachCallContext(callCtx, callId);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes: Buffer;
        try {
            resultBytes = rt.callFunctionSync(argsProto, ctx ?? null, nativeCollectors);
        } catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(withTypeMap(typeMap, () => decodeCallResult(resultBytes)));
    } finally {
        callCtxBinding.detach();
    }
}

export async function callFunction(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    ctx?: HostSpanManager,
    collectors?: Collector[],
    callCtx?: BamlCallContext,
): Promise<FunctionResult> {
    const typeMap = typeMapForRuntime(rt);
    const callId = newFunctionCall();
    const argsProto = withTypeMap(typeMap, () => encodeCallArgs(kwargs, { callId, functionName }));
    const callCtxBinding = attachCallContext(callCtx, callId);
    const nativeCollectors = collectors?.map(c => c._native()) ?? null;
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes: Buffer;
        try {
            resultBytes = await rt.callFunction(argsProto, ctx ?? null, nativeCollectors);
        } catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(withTypeMap(typeMap, () => decodeCallResult(resultBytes)));
    } finally {
        callCtxBinding.detach();
    }
}

// Register flush on process exit (single registration; see exit_hook.ts).
installFlushOnExit();
