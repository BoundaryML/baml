import { BamlRuntime, cancelFunctionCall as nativeCancelFunctionCall, getRuntime, installHostCallableDispatchFactory, newFunctionCall as nativeNewFunctionCall } from "./native.js";
import { decodeCallResult, encodeCallArgs, makeHostCallableDispatch } from "./shared/proto.js";
import { attachInvocation } from "./shared/call_context.js";

installHostCallableDispatchFactory(makeHostCallableDispatch);

export { BamlAudio, BamlHandle, BamlImage, BamlPdf, BamlRuntime, BamlVideo, _seedFunctionRefHandle, _seedGenericMediaHandle, getBridgeRuntimeVersion, getRuntime, getToolchainVersion, getVersion, newFunctionCall } from "./native.js";
export { BamlStream } from "./shared/stream.js";
export { BamlFunctionSpec } from "./shared/function_spec.js";
export type { BamlFunctionSpecBuildRequestOptions, BamlFunctionSpecCallOptions } from "./shared/function_spec.js";
export { BamlTypeMap, getTypeMap, setTypeMap } from "./shared/typemap.js";
export { defineFunction, defineInstanceFunction, UNSET } from "./shared/define_function.js";
export type { GenericParams } from "./shared/define_function.js";
export { Never, lowerTypeToWireTy } from "./shared/wire_ty.js";
export { BamlType, reflectType } from "./shared/wire_ty.js";
export type { BamlClassCtor, BamlInterfaceToken, BamlPrimitiveToken, BamlTypeMetadata, BamlTypeToken } from "./shared/wire_ty.js";
export { BamlAbortError, BamlCancelledError, BamlClientError, BamlError, BamlInvalidArgumentError, BamlPanic, wrapNativeError } from "./shared/errors.js";
export { BamlPrompt, decodeCallResult, encodeCallArgs } from "./shared/proto.js";
export type { BamlPromptCallOptions, BamlPromptMessage } from "./shared/proto.js";

export function initializeRuntimeFromBlob(bytecode: string | Uint8Array, embeddedBamlToml?: string): void {
  BamlRuntime.initializeRuntimeFromBlob(bytecode, embeddedBamlToml);
}

export function initializeRuntime(srcDir: string, files: Record<string, string>): void {
  BamlRuntime.initializeRuntime(srcDir, files);
}

export class FunctionResult {
  constructor(private readonly value: unknown) {}
  result(): unknown { return this.value; }
  toString(): string { return `FunctionResult(${JSON.stringify(this.value)})`; }
}

export function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, baml?: InvocationOptions | null): FunctionResult {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { syncMode: true, callId, functionName, baml });
  const callCtxBinding = attachInvocation(args, callId);
  try {
    return new FunctionResult(decodeCallResult(rt.callFunctionSync(args)));
  } finally {
    callCtxBinding.detach();
  }
}

export async function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, baml?: InvocationOptions | null): Promise<FunctionResult> {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { callId, functionName, baml });
  const callCtxBinding = attachInvocation(args, callId);
  try {
    return new FunctionResult(decodeCallResult(await rt.callFunction(args)));
  } finally {
    callCtxBinding.detach();
  }
}

export function cancelFunctionCall(callId: bigint): boolean {
  return nativeCancelFunctionCall(callId);
}

export { current as _currentExecutionContext, ExecutionContext as _ExecutionContext, currentContext as _currentTraceContext, currentContextAsync as _currentTraceContextAsync, currentCancelToken as _currentCancelToken, withExecutionContext as _withExecutionContext } from './shared/execution_context.js';
export { invoke as _invoke, invokeAsync as _invokeAsync } from './shared/invocation.js';
export { instrument as _instrument, TraceUsageError } from './instrumentation.js';
import type { InvocationOptions } from './shared/invocation.js';
