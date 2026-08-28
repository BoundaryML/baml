import { BamlCallContext, BamlRuntime, Collector, HostSpanManager, cancelFunctionCall as nativeCancelFunctionCall, getRuntime, installHostCallableDispatchFactory, newFunctionCall as nativeNewFunctionCall } from "./native.js";
import { decodeCallResult, encodeCallArgs, makeHostCallableDispatch, type FunctionOperation } from "./shared/proto.js";
import { attachCallContext } from "./shared/call_context.js";

installHostCallableDispatchFactory(makeHostCallableDispatch);

export { BamlAudio, BamlCallContext, BamlHandle, BamlImage, BamlPdf, BamlRuntime, BamlVideo, Collector, FunctionLog, HostSpanManager, Timing, Usage, _seedFunctionRefHandle, _seedGenericMediaHandle, flushEvents, getBridgeRuntimeVersion, getRuntime, getToolchainVersion, getVersion, newFunctionCall } from "./native.js";
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
export type { BamlPromptCallOptions, BamlPromptMessage, FunctionOperation } from "./shared/proto.js";

export function initializeRuntimeFromBytecode(bytecode: Uint8Array, embeddedBamlToml?: string): void {
  BamlRuntime.initializeRuntimeFromBytecode(bytecode, embeddedBamlToml);
}

export function initializeRuntime(srcDir: string, files: Record<string, string>): void {
  BamlRuntime.initializeRuntime(srcDir, files);
}

export class FunctionResult {
  constructor(private readonly value: unknown) {}
  result(): unknown { return this.value; }
  toString(): string { return `FunctionResult(${JSON.stringify(this.value)})`; }
}

export function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], callCtx?: BamlCallContext, operation: FunctionOperation = 'direct'): FunctionResult {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { syncMode: true, callId, functionName, operation });
  const callCtxBinding = attachCallContext(callCtx, callId);
  try {
    return new FunctionResult(decodeCallResult(rt.callFunctionSync(args, ctx ?? null, collectors ?? null)));
  } finally {
    callCtxBinding.detach();
  }
}

export async function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], callCtx?: BamlCallContext, operation: FunctionOperation = 'direct'): Promise<FunctionResult> {
  const callId = nativeNewFunctionCall();
  const args = encodeCallArgs(kwargs, { callId, functionName, operation });
  const callCtxBinding = attachCallContext(callCtx, callId);
  try {
    return new FunctionResult(decodeCallResult(await rt.callFunction(args, ctx ?? null, collectors ?? null)));
  } finally {
    callCtxBinding.detach();
  }
}

export class CtxManager {
  private readonly context = new HostSpanManager();
  get(): HostSpanManager { return this.context; }
  allowReset(): boolean { return true; }
  reset(): void {}
  cloneContext(): HostSpanManager { return this.context.deepClone(); }
  upsertTags(tags: Record<string, string>): void { this.context.upsertTags(tags); }
}

export function cancelFunctionCall(callId: bigint): boolean {
  return nativeCancelFunctionCall(callId);
}
