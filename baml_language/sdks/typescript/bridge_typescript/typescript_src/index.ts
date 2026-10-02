// index.ts — mirrors bridge_python/python_src/baml_py/__init__.py

import {
    BamlRuntime,
    BamlHandle,
    cancelFunctionCall as nativeCancelFunctionCall,
    newFunctionCall as nativeNewFunctionCall,
} from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
import { installShutdownOnExit } from './exit_hook.js';
import { wrapNativeError } from './errors.js';
import { attachInvocation } from './call_context.js';

export {
    BamlRuntime,
    BamlHandle,
    getRuntime,
    getBridgeRuntimeVersion,
    getToolchainVersion,
    getVersion,
} from './native.js';
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
export function initializeRuntime(srcDir: string, files: Record<string, string>): void {
    BamlRuntime.initializeRuntime(srcDir, files);
}

/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts` when
 * codegen embeds precompiled BAML bytecode.
 */
export function initializeRuntimeFromBlob(bytecode: string | Buffer | Uint8Array, embeddedBamlToml?: string): void {
    // Generated SDKs pass their embedded bytecode string through untouched;
    // the native bridge decodes it.
    BamlRuntime.initializeRuntimeFromBlob(
        typeof bytecode === "string" ? bytecode : Buffer.from(bytecode),
        embeddedBamlToml,
    );
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

export function callFunctionSync(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    baml?: InvocationOptions | null,
): FunctionResult {
    // Encode in sync mode so a host callable in the kwargs fast-fails
    // with a clear error instead of registering a tsfn and then hanging —
    // the sync path blocks the Node main thread on a tokio `block_on`,
    // starving libuv so the dispatch could never run.
    const callId = newFunctionCall();
    const argsProto = encodeCallArgs(kwargs, { syncMode: true, callId, functionName, baml });
    const callCtxBinding = attachInvocation(argsProto, callId);
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes: Buffer;
        try {
            resultBytes = rt.callFunctionSync(argsProto);
        } catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(decodeCallResult(resultBytes));
    } finally {
        callCtxBinding.detach();
    }
}

export async function callFunction(
    rt: BamlRuntime,
    functionName: string,
    kwargs: Record<string, unknown>,
    baml?: InvocationOptions | null,
): Promise<FunctionResult> {
    const callId = newFunctionCall();
    const argsProto = encodeCallArgs(kwargs, { callId, functionName, baml });
    const callCtxBinding = attachInvocation(argsProto, callId);
    // Only the napi call gets `wrapNativeError`'d — its `napi::Error`
    // messages need parsing into typed `Baml*Error` subclasses. The
    // decoder's throws (`BamlError`/`BamlPanic`, *or* a re-raised
    // original JS exception from the host-callable rehydration path)
    // already carry the right type and must propagate by identity.
    try {
        let resultBytes: Buffer;
        try {
            resultBytes = await rt.callFunction(argsProto);
        } catch (err) {
            throw wrapNativeError(err);
        }
        return new FunctionResult(decodeCallResult(resultBytes));
    } finally {
        callCtxBinding.detach();
    }
}

// Register runtime shutdown on process exit (single registration; see exit_hook.ts).
installShutdownOnExit();

export { current as _currentExecutionContext, ExecutionContext as _ExecutionContext, currentContext as _currentTraceContext, currentContextAsync as _currentTraceContextAsync, currentCancelToken as _currentCancelToken, withExecutionContext as _withExecutionContext } from './execution_context.js';
export { invoke as _invoke, invokeAsync as _invokeAsync } from './invocation.js';
export { instrument as _instrument, TraceUsageError } from './instrumentation.js';
import type { InvocationOptions } from './invocation.js';
